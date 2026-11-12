package mekanism.common.tests.network;

import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import it.unimi.dsi.fastutil.ints.Int2ObjectMap;
import it.unimi.dsi.fastutil.ints.Int2ObjectMaps;
import it.unimi.dsi.fastutil.longs.LongList;
import it.unimi.dsi.fastutil.objects.ObjectIterator;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import mekanism.api.SerializationConstants;
import mekanism.api.text.EnumColor;
import mekanism.common.Mekanism;
import mekanism.common.capabilities.Capabilities;
import mekanism.common.content.network.transmitter.DiversionTransporter;
import mekanism.common.content.network.transmitter.DiversionTransporter.DiversionControl;
import mekanism.common.content.network.transmitter.LogisticalTransporterBase;
import mekanism.common.content.network.transmitter.RestrictiveTransporter;
import mekanism.common.content.transporter.TransporterStack;
import mekanism.common.lib.transmitter.ConnectionType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tests.util.StructureBuilderUtils;
import mekanism.common.tile.transmitter.TileEntityLogisticalTransporterBase;
import mekanism.common.util.EnumUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.Tag;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.Container;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import net.neoforged.neoforge.transfer.ResourceHandler;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;
import org.jspecify.annotations.Nullable;

/// Grava, tick a tick, o movimento das pilhas nos transportadores logísticos do original (frente `transporte-movimento`, Fase 3), um `.jsonl` por cenário em
/// `trace/transporter_move_<cenário>.jsonl`, para `mods/machinery_basis/core/tests/transporter_move_reference.rs` repetir. Linhas:
///
/// - `setup`: a origem (importa para a ordem das tabelas do fastutil), cada transportador (tipo, nível, cor, conexões cruas, modos do desviador), cada
///   contêiner (`size` slots) e as pilhas pedidas;
/// - `mark`: a ação do cenário, sempre no `ServerTickEvent.Pre` do tick `t` (`fill`, `clear`, `power`, `break`, `cap_insert`), com o resultado quando há;
/// - `state`: depois do tick `t` (`ServerTickEvent.Post`), só quando algo mudou desde a linha anterior: cada transportador (`delay`, `delayCount`, `nextId`,
///   as pilhas em trânsito na ordem da tabela do `Int2ObjectOpenHashMap`, as pendentes de envio `sync`, o NBT salvo `nbt` e o de atualização `upd`), cada
///   pilha com o `System.identityHashCode`, o próximo e o anterior, o lado, o NBT e o `STREAM_CODEC` em hexadecimal; os contêineres (`chests`, só os que
///   mudaram) e os itens soltos no mundo (`drops`, vistos pela primeira vez);
/// - `begin` é o `state` de antes do primeiro tick.
///
/// Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace_transporter_move")
public class TransporterMoveTraceTest {

    private static final int SETUP = SharedConstants.TICKS_PER_SECOND;
    private static final int TIMEOUT = 40 * SharedConstants.TICKS_PER_SECOND;

    private static BlockState tier(int tier) {
        return switch (tier) {
            case 0 -> MekanismBlocks.BASIC_LOGISTICAL_TRANSPORTER.defaultState();
            case 1 -> MekanismBlocks.ADVANCED_LOGISTICAL_TRANSPORTER.defaultState();
            case 2 -> MekanismBlocks.ELITE_LOGISTICAL_TRANSPORTER.defaultState();
            default -> MekanismBlocks.ULTIMATE_LOGISTICAL_TRANSPORTER.defaultState();
        };
    }

    private static final Field TRANSIT = field("transit");
    private static final Field NEEDS_SYNC = field("needsSync");
    private static final Field NEXT_ID = field("nextId");
    private static final Field DELAY = field("delay");
    private static final Field DELAY_COUNT = field("delayCount");

    private static Field field(String name) {
        try {
            Field field = LogisticalTransporterBase.class.getDeclaredField(name);
            field.setAccessible(true);
            return field;
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    /// O que o cenário montou: onde ficam os transportadores e os contêineres.
    private static final class Layout {

        final List<int[]> transporters = new ArrayList<>();
        final List<int[]> containers = new ArrayList<>();
        final List<Integer> sizes = new ArrayList<>();

        Layout t(StructureTemplateBuilder builder, int x, int y, int z, BlockState state, @Nullable CompoundTag tag) {
            transporters.add(new int[]{x, y, z});
            builder.set(x, y, z, state, tag);
            return this;
        }

        Layout row(StructureTemplateBuilder builder, int x1, int x2, int y, int z, BlockState state, @Nullable CompoundTag firstTag) {
            for (int x = x1; x <= x2; x++) {
                t(builder, x, y, z, state, x == x1 ? firstTag : null);
            }
            return this;
        }

        Layout chest(StructureTemplateBuilder builder, int x, int y, int z) {
            containers.add(new int[]{x, y, z});
            sizes.add(27);
            builder.set(x, y, z, Blocks.CHEST.defaultBlockState());
            return this;
        }

        /// Um dropper de nove slots, o destino apertado.
        Layout dropper(StructureTemplateBuilder builder, int x, int y, int z) {
            containers.add(new int[]{x, y, z});
            sizes.add(9);
            builder.set(x, y, z, Blocks.DROPPER.defaultBlockState());
            return this;
        }
    }

    private static CompoundTag pull(Direction side) {
        return StructureBuilderUtils.configured(null, side, ConnectionType.PULL);
    }

    @FunctionalInterface
    private interface Action {

        /// Roda a ação e devolve os campos extras da linha `mark`.
        String run(Recorder recorder);
    }

    private record Scheduled(int tick, String kind, String fields, Action action) {
    }

    private static final class Recorder {

        @Nullable
        static volatile Recorder current;

        final String name;
        final MekGameTestHelper helper;
        final ServerLevel level;
        final BlockPos origin;
        final Layout layout;
        final long startTick;
        final List<String> lines = new ArrayList<>();
        final List<Scheduled> scheduled = new ArrayList<>();
        final Map<ItemEntity, Boolean> seen = new IdentityHashMap<>();
        String lastChests = "";
        boolean finished;
        boolean finishing;

        Recorder(String name, MekGameTestHelper helper, Layout layout) {
            this.name = name;
            this.helper = helper;
            this.level = helper.getLevel();
            this.origin = helper.absolutePos(BlockPos.ZERO);
            this.layout = layout;
            this.startTick = ServerLifecycleHooks.getCurrentServer().getTickCount();
        }

        static void begin(String name, MekGameTestHelper helper, Layout layout) {
            Recorder recorder = new Recorder(name, helper, layout);
            recorder.setup();
            current = recorder;
            recorder.record("begin", 0);
        }

        void schedule(int tick, String kind, String fields, Action action) {
            scheduled.add(new Scheduled(tick, kind, fields, action));
        }

        static void pre() {
            Recorder recorder = current;
            if (recorder == null || recorder.finished) {
                return;
            }
            int tick = recorder.relativeTick();
            for (Scheduled task : new ArrayList<>(recorder.scheduled)) {
                if (task.tick() == tick) {
                    recorder.scheduled.remove(task);
                    String extra = task.action().run(recorder);
                    recorder.lines.add("{\"e\":\"mark\",\"t\":" + tick + ",\"k\":\"" + task.kind() + "\"," + task.fields() + (extra.isEmpty() ? "" : "," + extra) + "}");
                }
            }
        }

        static void post() {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished) {
                recorder.record("state", recorder.relativeTick());
                if (recorder.finishing) {
                    recorder.finish();
                }
            }
        }

        int relativeTick() {
            return (int) (ServerLifecycleHooks.getCurrentServer().getTickCount() - startTick);
        }

        void finish() {
            finished = true;
            lines.add("{\"e\":\"mark\",\"t\":" + relativeTick() + ",\"k\":\"finish\"}");
            try {
                Path file = Path.of("trace", "transporter_move_" + name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }

        // ---------------------------------------------------------------------------------------------------------------------------------------------

        String rel(BlockPos pos) {
            return "[" + (pos.getX() - origin.getX()) + "," + (pos.getY() - origin.getY()) + "," + (pos.getZ() - origin.getZ()) + "]";
        }

        String rel(long packed) {
            return packed == Long.MAX_VALUE ? "null" : rel(BlockPos.of(packed));
        }

        static String rel(int[] p) {
            return "[" + p[0] + "," + p[1] + "," + p[2] + "]";
        }

        String path(LongList list) {
            StringBuilder out = new StringBuilder("[");
            for (int i = 0; i < list.size(); i++) {
                out.append(i == 0 ? "" : ",").append(rel(list.getLong(i)));
            }
            return out.append("]").toString();
        }

        static String dir(@Nullable Direction side) {
            return side == null ? "null" : "\"" + side.getName() + "\"";
        }

        static String color(@Nullable EnumColor color) {
            return color == null ? "null" : "\"" + color.getSerializedName() + "\"";
        }

        static String item(Item item) {
            return "\"" + BuiltInRegistries.ITEM.getKey(item) + "\"";
        }

        static String quote(String text) {
            return "\"" + text.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
        }

        BlockPos abs(int[] p) {
            return origin.offset(p[0], p[1], p[2]);
        }

        LogisticalTransporterBase transporter(int[] p) {
            return helper.getBlockEntity(p[0], p[1], p[2], TileEntityLogisticalTransporterBase.class).getTransmitter();
        }

        Container container(int[] p) {
            return (Container) level.getBlockEntity(abs(p));
        }

        void setup() {
            StringBuilder ts = new StringBuilder();
            for (int[] p : layout.transporters) {
                LogisticalTransporterBase t = transporter(p);
                String kind = t instanceof DiversionTransporter ? "diversion" : t instanceof RestrictiveTransporter ? "restrictive" : "logistical";
                StringBuilder raw = new StringBuilder();
                StringBuilder modes = new StringBuilder();
                for (Direction side : EnumUtils.DIRECTIONS) {
                    String sep = side.ordinal() == 0 ? "" : ",";
                    raw.append(sep).append(t.getConnectionTypeRaw(side).ordinal());
                    if (t instanceof DiversionTransporter diversion) {
                        modes.append(sep).append(diversion.modes[side.ordinal()].ordinal());
                    }
                }
                ts.append(ts.isEmpty() ? "" : ",").append("{\"p\":").append(rel(p)).append(",\"kind\":\"").append(kind).append("\",\"tier\":").append(t.tier.ordinal())
                      .append(",\"color\":").append(color(t.getColor())).append(",\"raw\":[").append(raw).append("]")
                      .append(kind.equals("diversion") ? ",\"modes\":[" + modes + "]" : "").append("}");
            }
            StringBuilder cs = new StringBuilder();
            for (int i = 0; i < layout.containers.size(); i++) {
                cs.append(i == 0 ? "" : ",").append("{\"p\":").append(rel(layout.containers.get(i))).append(",\"size\":").append(layout.sizes.get(i)).append("}");
            }
            lines.add("{\"e\":\"setup\",\"name\":\"" + name + "\",\"x\":" + origin.getX() + ",\"y\":" + origin.getY() + ",\"z\":" + origin.getZ()
                      + ",\"transporters\":[" + ts + "],\"containers\":[" + cs + "]}");
        }

        String containersJson() {
            StringBuilder out = new StringBuilder("[");
            for (int i = 0; i < layout.containers.size(); i++) {
                Container container = container(layout.containers.get(i));
                StringBuilder slots = new StringBuilder();
                for (int s = 0; s < container.getContainerSize(); s++) {
                    ItemStack stack = container.getItem(s);
                    if (!stack.isEmpty()) {
                        slots.append(slots.isEmpty() ? "" : ",").append("[").append(s).append(",").append(item(stack.getItem())).append(",").append(stack.getCount()).append("]");
                    }
                }
                out.append(i == 0 ? "" : ",").append("{\"p\":").append(rel(layout.containers.get(i))).append(",\"slots\":[").append(slots).append("]}");
            }
            return out.append("]").toString();
        }

        /// A pilha inteira na primeira vez que aparece e quando algo além do progresso muda; depois só `id`, `h` e `prog` (`s`: o resto é o da última vez).
        String stackJson(LogisticalTransporterBase t, int[] p, int id, TransporterStack stack, boolean emitStream) {
            long pos = t.getBlockPos().asLong();
            boolean empty = stack.isEmpty();
            String fixed = ",\"item\":" + (empty ? "null" : item(stack.getItemType().toStack(1).getItem())) + ",\"n\":" + stack.size()
                           + ",\"color\":" + color(stack.color) + ",\"init\":" + stack.initiatedPath
                           + ",\"idle\":" + dir(stack.idleDir) + ",\"orig\":" + rel(stack.originalLocation) + ",\"home\":" + rel(stack.homeLocation)
                           + ",\"pt\":\"" + stack.getPathType().getSerializedName() + "\",\"path\":" + path(stack.getPath())
                           + ",\"next\":" + rel(stack.getNext(t)) + ",\"prev\":" + rel(stack.getPrev(t)) + ",\"side\":" + dir(stack.getSide(t));
            String key = rel(p) + "|" + id + "|" + System.identityHashCode(stack);
            boolean same = fixed.equals(lastStatic.get(key));
            lastStatic.put(key, fixed);
            StringBuilder out = new StringBuilder("{\"id\":").append(id).append(",\"h\":").append(System.identityHashCode(stack)).append(",\"prog\":").append(stack.progress);
            if (same) {
                out.append(",\"s\":true");
            } else {
                out.append(fixed);
            }
            if (!empty && emitStream) {
                stack.updateForPos(pos);
                RegistryFriendlyByteBuf buf = new RegistryFriendlyByteBuf(Unpooled.buffer(), level.registryAccess());
                TransporterStack.STREAM_CODEC.encode(buf, stack);
                out.append(",\"iid\":").append(BuiltInRegistries.ITEM.getId(stack.getItemType().toStack(1).getItem())).append(",\"stream\":\"").append(ByteBufUtil.hexDump(buf)).append("\"");
            }
            return out.append("}").toString();
        }

        final Map<String, String> lastStatic = new java.util.HashMap<>();
        final Map<String, String> lastCore = new java.util.HashMap<>();

        /// O transportador sem o `delay` (que vai à parte, em todo tick): o resto só entra na linha se mudou.
        @SuppressWarnings("unchecked")
        String transporterJson(int[] p, boolean emitNbt) {
            LogisticalTransporterBase t = transporter(p);
            try {
                Int2ObjectMap<TransporterStack> transit = (Int2ObjectMap<TransporterStack>) TRANSIT.get(t);
                Int2ObjectMap<TransporterStack> needsSync = (Int2ObjectMap<TransporterStack>) NEEDS_SYNC.get(t);
                StringBuilder stacks = new StringBuilder();
                for (ObjectIterator<Int2ObjectMap.Entry<TransporterStack>> it = Int2ObjectMaps.fastIterator(transit); it.hasNext(); ) {
                    Int2ObjectMap.Entry<TransporterStack> entry = it.next();
                    stacks.append(stacks.isEmpty() ? "" : ",").append(stackJson(t, p, entry.getIntKey(), entry.getValue(), emitNbt));
                }
                StringBuilder sync = new StringBuilder();
                for (ObjectIterator<Int2ObjectMap.Entry<TransporterStack>> it = Int2ObjectMaps.fastIterator(needsSync); it.hasNext(); ) {
                    sync.append(sync.isEmpty() ? "" : ",").append(it.next().getIntKey());
                }
                String nbt = "";
                if (emitNbt && !transit.isEmpty()) {
                    try (ProblemReporter.ScopedCollector reporter = new ProblemReporter.ScopedCollector(Mekanism.logger)) {
                        TagValueOutput output = TagValueOutput.createWithContext(reporter, level.registryAccess());
                        t.write(output);
                        Tag items = output.buildResult().get(SerializationConstants.ITEMS);
                        TagValueOutput update = TagValueOutput.createWithContext(reporter, level.registryAccess());
                        t.writeReducedUpdatedTag(update);
                        Tag updItems = update.buildResult().get(SerializationConstants.ITEMS);
                        nbt = ",\"nbt\":" + (items == null ? "null" : quote(items.toString())) + ",\"upd\":" + (updItems == null ? "null" : quote(updItems.toString()));
                    }
                }
                return "{\"p\":" + rel(p) + ",\"dc\":" + DELAY_COUNT.getInt(t) + ",\"nid\":" + NEXT_ID.getInt(t) + ",\"transit\":[" + stacks + "],\"sync\":[" + sync + "]" + nbt + "}";
            } catch (IllegalAccessException e) {
                throw new IllegalStateException(e);
            }
        }

        void record(String kind, int tick) {
            boolean emitNbt = tick % 5 == 0;
            StringBuilder ts = new StringBuilder();
            StringBuilder delays = new StringBuilder();
            for (int[] p : layout.transporters) {
                BlockPos pos = abs(p);
                delays.append(delays.isEmpty() ? "" : ",");
                String core;
                if (!(level.getBlockEntity(pos) instanceof TileEntityLogisticalTransporterBase)) {
                    core = "{\"p\":" + rel(p) + ",\"gone\":true}";
                    delays.append("-1");
                } else {
                    core = transporterJson(p, emitNbt);
                    try {
                        delays.append(DELAY.getInt(transporter(p)));
                    } catch (IllegalAccessException e) {
                        throw new IllegalStateException(e);
                    }
                }
                String key = rel(p);
                if (!core.equals(lastCore.get(key))) {
                    lastCore.put(key, core);
                    ts.append(ts.isEmpty() ? "" : ",").append(core);
                }
            }
            String chests = containersJson();
            // Itens soltos vistos pela primeira vez.
            List<ItemEntity> drops = new ArrayList<>();
            for (ItemEntity entity : level.getEntitiesOfClass(ItemEntity.class, new AABB(origin).inflate(24))) {
                if (seen.put(entity, Boolean.TRUE) == null) {
                    drops.add(entity);
                }
            }
            StringBuilder dropJson = new StringBuilder();
            for (ItemEntity entity : drops) {
                BlockPos at = entity.blockPosition();
                dropJson.append(dropJson.isEmpty() ? "" : ",").append("{\"item\":").append(item(entity.getItem().getItem())).append(",\"n\":").append(entity.getItem().getCount())
                      .append(",\"p\":").append(rel(at)).append("}");
            }
            StringBuilder line = new StringBuilder("{\"e\":\"").append(kind).append("\",\"t\":").append(tick).append(",\"dl\":[").append(delays).append("],\"tx\":[").append(ts).append("]");
            if (!chests.equals(lastChests)) {
                line.append(",\"chests\":").append(chests);
            }
            if (!drops.isEmpty()) {
                line.append(",\"drops\":[").append(dropJson).append("]");
            }
            lines.add(line.append("}").toString());
            lastChests = chests;
        }
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Ações dos cenários
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    private static String pos(int[] p) {
        return "\"p\":" + Recorder.rel(p);
    }

    private static void fill(Recorder r, int tick, int[] p, int slot, Item item, int count) {
        r.schedule(tick, "fill", pos(p) + ",\"slot\":" + slot + ",\"item\":" + Recorder.item(item) + ",\"n\":" + count, rec -> {
            rec.container(p).setItem(slot, new ItemStack(item, count));
            return "";
        });
    }

    private static void fillAll(Recorder r, int tick, int[] p, Item item, int count) {
        r.schedule(tick, "fill_all", pos(p) + ",\"item\":" + Recorder.item(item) + ",\"n\":" + count, rec -> {
            Container container = rec.container(p);
            for (int i = 0; i < container.getContainerSize(); i++) {
                container.setItem(i, new ItemStack(item, count));
            }
            return "";
        });
    }

    private static void power(Recorder r, int tick, int[] p, boolean on) {
        r.schedule(tick, "power", pos(p) + ",\"on\":" + on, rec -> {
            rec.helper.setBlock(p[0], p[1], p[2], on ? Blocks.REDSTONE_BLOCK.defaultBlockState() : Blocks.AIR.defaultBlockState());
            return "";
        });
    }

    private static void breakAt(Recorder r, int tick, int[] p) {
        r.schedule(tick, "break", pos(p), rec -> {
            rec.helper.setBlock(p[0], p[1], p[2], Blocks.AIR.defaultBlockState());
            return "";
        });
    }

    /// Insere pela capability de item do transportador, como um funil ou um ejetor de outro mod faria.
    private static void capInsert(Recorder r, int tick, int[] p, Direction side, Item item, int count) {
        r.schedule(tick, "cap_insert", pos(p) + ",\"side\":" + Recorder.dir(side) + ",\"item\":" + Recorder.item(item) + ",\"n\":" + count, rec -> {
            ResourceHandler<ItemResource> handler = rec.helper.getCapability(Capabilities.ITEM.block(), new BlockPos(p[0], p[1], p[2]), side);
            int inserted = -1;
            if (handler != null) {
                try (Transaction transaction = Transaction.openRoot()) {
                    inserted = handler.insert(ItemResource.of(new ItemStack(item)), count, transaction);
                    transaction.commit();
                }
            }
            return "\"inserted\":" + inserted;
        });
    }

    private static void run(DynamicTest test, String name, Layout layout, int ticks, Setup setup) {
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> Recorder.pre());
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> Recorder.post());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin(name, helper, layout);
                  Recorder recorder = Recorder.current;
                  setup.accept(recorder);
                  recorder.schedule(ticks, "finishing", "\"x\":0", rec -> {
                      rec.finishing = true;
                      return "";
                  });
              })
              .thenWaitUntil(() -> {
                  Recorder recorder = Recorder.current;
                  if (recorder == null || !recorder.finished) {
                      helper.fail("Trace not finished yet");
                  }
              })
              .thenSucceed());
    }

    @FunctionalInterface
    private interface Setup {

        void accept(Recorder recorder);
    }

    private static final int[] NONE = {0, 0, 0};

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Um baú que o transportador puxa para outro, por uma linha básica
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_move_pull_line")
    @TestHolder(description = "Records the stacks a pulling basic transporter moves along a line to a chest.")
    public static void pullLine(final DynamicTest test) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(8, 1, 1);
            layout.chest(builder, 0, 0, 0).row(builder, 1, 5, 0, 0, tier(0), pull(Direction.WEST)).chest(builder, 6, 0, 0);
            return builder;
        });
        run(test, "pull_line", layout, 330, r -> {
            // Os itens da origem entram a esta altura, com o puxar já rodando em vazio.
            fill(r, 2, new int[]{0, 0, 0}, 0, Items.DIRT, 64);
            fill(r, 2, new int[]{0, 0, 0}, 1, Items.GRAVEL, 3);
            fill(r, 2, new int[]{0, 0, 0}, 5, Items.STONE, 2);
        });
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Os quatro níveis, quatro linhas lado a lado
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_move_tiers")
    @TestHolder(description = "Records the four transporter tiers pulling from a chest to a chest.")
    public static void tiers(final DynamicTest test) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(8, 1, 7);
            for (int tier = 0; tier < 4; tier++) {
                int z = tier * 2;
                layout.chest(builder, 0, 0, z).row(builder, 1, 5, 0, z, tier(tier), pull(Direction.WEST)).chest(builder, 6, 0, z);
            }
            return builder;
        });
        run(test, "tiers", layout, 160, r -> {
            for (int tier = 0; tier < 4; tier++) {
                fill(r, 2, new int[]{0, 0, tier * 2}, 0, Items.DIRT, 64);
                fill(r, 2, new int[]{0, 0, tier * 2}, 1, Items.GRAVEL, 40);
            }
        });
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // O destino que enche no meio do caminho
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    private static void fillsMidway(final DynamicTest test, String name, boolean sourceFull) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(8, 1, 1);
            layout.chest(builder, 0, 0, 0).row(builder, 1, 5, 0, 0, tier(1), pull(Direction.WEST)).dropper(builder, 6, 0, 0);
            return builder;
        });
        run(test, name, layout, 260, r -> {
            fill(r, 2, new int[]{0, 0, 0}, 0, Items.DIRT, 16);
            // O destino tem um slot livre, que o puxar de 16 ocupa; aos 10 ticks outro item toma esse slot.
            for (int slot = 0; slot < 8; slot++) {
                fill(r, 1, new int[]{6, 0, 0}, slot, Items.COBBLESTONE, 64);
            }
            fill(r, 12, new int[]{6, 0, 0}, 8, Items.STONE, 64);
            if (sourceFull) {
                // A casa também fica cheia: a pilha não tem para onde voltar e passeia pelo transportador.
                for (int slot = 0; slot < 27; slot++) {
                    fill(r, 12, new int[]{0, 0, 0}, slot, Items.COBBLESTONE, 64);
                }
            }
        });
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_move_full_midway")
    @TestHolder(description = "Records a stack whose destination fills up while it travels, and goes back home.")
    public static void fullMidway(final DynamicTest test) {
        fillsMidway(test, "full_midway", false);
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_move_full_midway_idle")
    @TestHolder(description = "Records a stack whose destination and home both fill up, so it idles.")
    public static void fullMidwayIdle(final DynamicTest test) {
        fillsMidway(test, "full_midway_idle", true);
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // O transportador quebrado com pilhas dentro
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_move_broken")
    @TestHolder(description = "Records a transporter broken with stacks inside, and the stacks around it.")
    public static void broken(final DynamicTest test) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(8, 1, 1);
            layout.chest(builder, 0, 0, 0).row(builder, 1, 5, 0, 0, tier(1), pull(Direction.WEST)).chest(builder, 6, 0, 0);
            return builder;
        });
        run(test, "broken", layout, 220, r -> {
            fill(r, 2, new int[]{0, 0, 0}, 0, Items.DIRT, 64);
            fill(r, 2, new int[]{0, 0, 0}, 1, Items.GRAVEL, 64);
            // Aos 24 ticks há pilhas em dois transportadores; quebra o do meio.
            breakAt(r, 24, new int[]{3, 0, 0});
        });
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Duas pilhas de itens diferentes para o mesmo destino, onde uma não cabe mais (a ordem do conjunto das pilhas em voo)
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    private static void contested(final DynamicTest test, String name) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(11, 2, 1);
            layout.chest(builder, 0, 0, 0).row(builder, 1, 4, 0, 0, tier(0), pull(Direction.WEST)).dropper(builder, 5, 0, 0);
            layout.row(builder, 6, 9, 0, 0, tier(0), null).chest(builder, 10, 0, 0);
            // Os transportadores do leste puxam do baú do leste: o primeiro do ramo (x = 9) puxa para o leste.
            builder.set(9, 0, 0, tier(0), pull(Direction.EAST));
            // O terceiro baú, em cima do segundo transportador do oeste, que o puxa de cima.
            layout.chest(builder, 2, 1, 0);
            builder.set(2, 0, 0, tier(0), pull(Direction.UP));
            return builder;
        });
        run(test, name, layout, 150, r -> {
            fill(r, 2, new int[]{0, 0, 0}, 0, Items.DIRT, 8);
            fill(r, 2, new int[]{10, 0, 0}, 0, Items.GRAVEL, 8);
            // O destino: sete slots de pedra lisa, dois livres; aos 8 ticks, com as duas pilhas já em voo, um dos livres é tomado.
            for (int slot = 0; slot < 7; slot++) {
                fill(r, 1, new int[]{5, 0, 0}, slot, Items.COBBLESTONE, 64);
            }
            fill(r, 8, new int[]{5, 0, 0}, 7, Items.STONE, 64);
            // O terceiro baú recebe terra: o puxar dele já está rodando em vazio.
            fill(r, 10, new int[]{2, 1, 0}, 0, Items.DIRT, 4);
        });
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_move_contested_1")
    @TestHolder(description = "Records two in-flight stacks of different items to one destination that shrinks, then a third request (first run).")
    public static void contested1(final DynamicTest test) {
        contested(test, "contested_1");
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_move_contested_2")
    @TestHolder(description = "Same as contested_1 (second run, other identity hash codes).")
    public static void contested2(final DynamicTest test) {
        contested(test, "contested_2");
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_move_contested_3")
    @TestHolder(description = "Same as contested_1 (third run, other identity hash codes).")
    public static void contested3(final DynamicTest test) {
        contested(test, "contested_3");
    }

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_move_contested_4")
    @TestHolder(description = "Same as contested_1 (fourth run, other identity hash codes).")
    public static void contested4(final DynamicTest test) {
        contested(test, "contested_4");
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // Inserir pela capability de item (um funil, o ejetor de outro bloco)
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_move_capability")
    @TestHolder(description = "Records items inserted into a transporter through its item capability.")
    public static void capability(final DynamicTest test) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(7, 1, 1);
            layout.chest(builder, 0, 0, 0).row(builder, 1, 4, 0, 0, tier(1), null).chest(builder, 5, 0, 0);
            return builder;
        });
        run(test, "capability", layout, 140, r -> {
            capInsert(r, 2, new int[]{1, 0, 0}, Direction.WEST, Items.DIRT, 5);
            // Mais do que o puxar do nível (16) e um item que não cabe em lugar nenhum: sem destino nenhum, o pedido não entra.
            capInsert(r, 30, new int[]{1, 0, 0}, Direction.WEST, Items.GRAVEL, 40);
            for (int slot = 0; slot < 27; slot++) {
                fill(r, 60, new int[]{5, 0, 0}, slot, Items.COBBLESTONE, 64);
            }
            capInsert(r, 62, new int[]{1, 0, 0}, Direction.WEST, Items.STONE, 3);
        });
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // O desviador mudando com o sinal
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_move_diversion")
    @TestHolder(description = "Records a diversion transporter whose branch opens and closes with the redstone signal.")
    public static void diversion(final DynamicTest test) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(8, 2, 3);
            layout.chest(builder, 0, 1, 2).row(builder, 1, 2, 1, 2, tier(0), pull(Direction.WEST))
                  .t(builder, 3, 1, 2, MekanismBlocks.DIVERSION_TRANSPORTER.defaultState(),
                        StructureBuilderUtils.diversionModes(null, DiversionControl.DISABLED, DiversionControl.DISABLED, DiversionControl.HIGH, DiversionControl.DISABLED,
                              DiversionControl.DISABLED, DiversionControl.LOW))
                  .row(builder, 4, 5, 1, 2, tier(0), null).chest(builder, 6, 1, 2);
            // O ramo do norte, que só liga com sinal.
            layout.t(builder, 3, 1, 1, tier(0), null).chest(builder, 3, 1, 0);
            return builder;
        });
        run(test, "diversion", layout, 300, r -> {
            fill(r, 2, new int[]{0, 1, 2}, 0, Items.DIRT, 64);
            power(r, 70, new int[]{3, 0, 2}, true);
            power(r, 190, new int[]{3, 0, 2}, false);
        });
    }

    // -----------------------------------------------------------------------------------------------------------------------------------------------------
    // O transportador restritivo no caminho
    // -----------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_move_restrictive")
    @TestHolder(description = "Records the stacks going through a restrictive transporter in the middle of a line.")
    public static void restrictive(final DynamicTest test) {
        Layout layout = new Layout();
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(8, 1, 1);
            layout.chest(builder, 0, 0, 0).row(builder, 1, 2, 0, 0, tier(2), pull(Direction.WEST))
                  .t(builder, 3, 0, 0, MekanismBlocks.RESTRICTIVE_TRANSPORTER.defaultState(), null)
                  .row(builder, 4, 5, 0, 0, tier(2), null).chest(builder, 6, 0, 0);
            return builder;
        });
        run(test, "restrictive", layout, 120, r -> fill(r, 2, new int[]{0, 0, 0}, 0, Items.DIRT, 64));
    }

}
