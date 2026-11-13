package mekanism.common.tests.network;

import it.unimi.dsi.fastutil.ints.Int2ObjectMap;
import it.unimi.dsi.fastutil.ints.Int2ObjectMaps;
import it.unimi.dsi.fastutil.objects.ObjectIterator;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.common.Mekanism;
import mekanism.common.content.network.transmitter.LogisticalTransporterBase;
import mekanism.common.content.transporter.TransporterStack;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismDataComponents;
import mekanism.common.registries.MekanismItems;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.transmitter.TileEntityLogisticalTransporterBase;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.component.DataComponents;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.Container;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.HopperBlock;
import net.minecraft.world.level.block.entity.HopperBlockEntity;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.TagValueOutput;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Dois rastros da frente `transportador-ajustes` (Fase 3):
///
/// - `trace/transporter_components.jsonl`: o `ItemResource.CODEC` e o `TransporterStack.write`/`writeToUpdateTag`/`read` de pilhas com componentes (uma ferramenta gasta e nomeada, um item
///   nomeado, um item do Mekanism com energia, um item sem componentes), uma linha por pilha: `res` (o `ItemResource.CODEC` como SNBT), `n`, `write`, `upd` (o SNBT de cada) e `back` (se
///   `read` devolve o mesmo recurso e a mesma quantidade);
/// - `trace/transporter_hopper.jsonl`: um funil com um baú em cima sobre o primeiro de quatro transportadores básicos, com um baú no fim da linha; uma linha por tick (`Post`) com o
///   que há no baú de cima, no funil (e o `cooldownTime` dele), no baú do fim e as pilhas em trânsito de cada transportador (id, item, quantidade e progresso).
///
/// Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace_transporter_ajustes")
public class TransporterAjustesTraceTest {

    private static final int SETUP = SharedConstants.TICKS_PER_SECOND;
    private static final int TIMEOUT = 40 * SharedConstants.TICKS_PER_SECOND;

    private static String quote(String text) {
        return "\"" + text.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
    }

    private static void write(String name, List<String> lines) {
        try {
            Path file = Path.of("trace", name);
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Os componentes do item na pilha salva
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_ajustes_components")
    @TestHolder(description = "Records how a transporter stack writes and reads items with data components.")
    public static void components(final DynamicTest test) {
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(1, 1, 1);
            builder.set(0, 0, 0, MekanismBlocks.BASIC_LOGISTICAL_TRANSPORTER.defaultState());
            return builder;
        });
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            ServerLevel level = helper.getLevel();
            LogisticalTransporterBase transporter = helper.getBlockEntity(0, 0, 0, TileEntityLogisticalTransporterBase.class).getTransmitter();
            ItemStack pickaxe = new ItemStack(Items.IRON_PICKAXE);
            pickaxe.setDamageValue(40);
            pickaxe.set(DataComponents.CUSTOM_NAME, Component.literal("Rápida"));
            ItemStack named = new ItemStack(Items.DIRT);
            named.set(DataComponents.CUSTOM_NAME, Component.literal("Terra \"boa\""));
            ItemStack tablet = new ItemStack(MekanismItems.ENERGY_TABLET.get());
            tablet.set(MekanismDataComponents.ATTACHED_ENERGY, 1000L);
            ItemStack plain = new ItemStack(Items.GRAVEL);
            List<String> lines = new ArrayList<>();
            ItemStack[] stacks = {pickaxe, named, tablet, plain};
            int[] amounts = {1, 5, 1, 16};
            for (int i = 0; i < stacks.length; i++) {
                ItemResource resource = ItemResource.of(stacks[i]);
                TransporterStack stack = new TransporterStack();
                stack.progress = 37;
                stack.originalLocation = helper.absolutePos(new BlockPos(0, 0, 0)).asLong();
                stack.setStack(resource, amounts[i]);
                String res = ItemResource.CODEC.encodeStart(level.registryAccess().createSerializationContext(NbtOps.INSTANCE), resource).getOrThrow().toString();
                try (ProblemReporter.ScopedCollector reporter = new ProblemReporter.ScopedCollector(Mekanism.logger)) {
                    TagValueOutput out = TagValueOutput.createWithContext(reporter, level.registryAccess());
                    stack.write(out);
                    CompoundTag written = out.buildResult();
                    TagValueOutput update = TagValueOutput.createWithContext(reporter, level.registryAccess());
                    stack.writeToUpdateTag(transporter, update);
                    CompoundTag updated = update.buildResult();
                    TransporterStack back = TransporterStack.read(TagValueInput.create(reporter, level.registryAccess(), written));
                    TransporterStack backUpdate = TransporterStack.readFromUpdate(TagValueInput.create(reporter, level.registryAccess(), updated));
                    boolean same = back.getItemType().equals(resource) && back.size() == amounts[i] && backUpdate.getItemType().equals(resource) && backUpdate.size() == amounts[i];
                    lines.add("{\"i\":" + i + ",\"res\":" + quote(res) + ",\"n\":" + amounts[i] + ",\"write\":" + quote(written.toString()) + ",\"upd\":" + quote(updated.toString())
                              + ",\"back\":" + same + "}");
                }
            }
            write("transporter_components.jsonl", lines);
        }).thenSucceed());
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // O funil que insere no transportador
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    private static final Field TRANSIT = field(LogisticalTransporterBase.class, "transit");
    private static final Field COOLDOWN = field(HopperBlockEntity.class, "cooldownTime");

    private static Field field(Class<?> type, String name) {
        try {
            Field field = type.getDeclaredField(name);
            field.setAccessible(true);
            return field;
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    private static final class Hopper {

        static volatile Hopper current;
        final MekGameTestHelper helper;
        final long startTick = ServerLifecycleHooks.getCurrentServer().getTickCount();
        final List<String> lines = new ArrayList<>();
        boolean finished;

        Hopper(MekGameTestHelper helper) {
            this.helper = helper;
        }

        int relativeTick() {
            return (int) (ServerLifecycleHooks.getCurrentServer().getTickCount() - startTick);
        }

        int count(Container container) {
            int total = 0;
            for (int i = 0; i < container.getContainerSize(); i++) {
                total += container.getItem(i).getCount();
            }
            return total;
        }

        void pre() {
            if (relativeTick() == 2) {
                Container top = (Container) helper.getBlockEntity(1, 2, 0, net.minecraft.world.level.block.entity.BlockEntity.class);
                top.setItem(0, new ItemStack(Items.DIRT, 64));
                lines.add("{\"e\":\"fill\",\"t\":2,\"n\":64}");
            }
        }

        @SuppressWarnings("unchecked")
        void post() {
            int tick = relativeTick();
            if (finished || tick < 0) {
                return;
            }
            try {
                Container top = (Container) helper.getBlockEntity(1, 2, 0, net.minecraft.world.level.block.entity.BlockEntity.class);
                HopperBlockEntity hopper = helper.getBlockEntity(1, 1, 0, HopperBlockEntity.class);
                Container dest = (Container) helper.getBlockEntity(5, 0, 0, net.minecraft.world.level.block.entity.BlockEntity.class);
                StringBuilder tx = new StringBuilder();
                for (int x = 1; x <= 4; x++) {
                    LogisticalTransporterBase t = helper.getBlockEntity(x, 0, 0, TileEntityLogisticalTransporterBase.class).getTransmitter();
                    Int2ObjectMap<TransporterStack> transit = (Int2ObjectMap<TransporterStack>) TRANSIT.get(t);
                    StringBuilder stacks = new StringBuilder();
                    for (ObjectIterator<Int2ObjectMap.Entry<TransporterStack>> it = Int2ObjectMaps.fastIterator(transit); it.hasNext(); ) {
                        Int2ObjectMap.Entry<TransporterStack> entry = it.next();
                        stacks.append(stacks.isEmpty() ? "" : ",").append("[").append(entry.getIntKey()).append(",").append(entry.getValue().size()).append(",").append(entry.getValue().progress).append("]");
                    }
                    tx.append(tx.isEmpty() ? "" : ",").append("[").append(stacks).append("]");
                }
                lines.add("{\"e\":\"state\",\"t\":" + tick + ",\"top\":" + count(top) + ",\"hop\":" + count(hopper) + ",\"cd\":" + COOLDOWN.getInt(hopper) + ",\"dest\":" + count(dest)
                          + ",\"tx\":[" + tx + "]}");
            } catch (IllegalAccessException e) {
                throw new IllegalStateException(e);
            }
            if (tick >= 140) {
                finished = true;
                write("transporter_hopper.jsonl", lines);
            }
        }
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_ajustes_hopper")
    @TestHolder(description = "Records a hopper feeding a line of basic transporters, tick by tick.")
    public static void hopper(final DynamicTest test) {
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(6, 3, 1);
            for (int x = 1; x <= 4; x++) {
                builder.set(x, 0, 0, MekanismBlocks.BASIC_LOGISTICAL_TRANSPORTER.defaultState());
            }
            builder.set(5, 0, 0, Blocks.CHEST.defaultBlockState());
            builder.set(1, 1, 0, Blocks.HOPPER.defaultBlockState().setValue(HopperBlock.FACING, net.minecraft.core.Direction.DOWN));
            builder.set(1, 2, 0, Blocks.CHEST.defaultBlockState());
            return builder;
        });
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> {
            Hopper hopper = Hopper.current;
            if (hopper != null && !hopper.finished) {
                hopper.pre();
            }
        });
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> {
            Hopper hopper = Hopper.current;
            if (hopper != null && !hopper.finished) {
                hopper.post();
            }
        });
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> Hopper.current = new Hopper(helper))
              .thenWaitUntil(() -> {
                  Hopper hopper = Hopper.current;
                  if (hopper == null || !hopper.finished) {
                      helper.fail("Trace not finished yet");
                  }
              })
              .thenSucceed());
    }
}
