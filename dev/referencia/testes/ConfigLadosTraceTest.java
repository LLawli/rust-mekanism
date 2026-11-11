package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.Codec;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;
import mekanism.api.RelativeSide;
import mekanism.api.text.EnumColor;
import mekanism.common.block.attribute.Attribute;
import mekanism.common.capabilities.Capabilities;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.capabilities.holder.IHolder;
import mekanism.common.capabilities.resolver.BasicSidedCapabilityResolver;
import mekanism.common.capabilities.resolver.manager.ResourceHandlerManager;
import mekanism.common.component.ConfigurationData;
import mekanism.common.component.component.AttachedEjector;
import mekanism.common.component.component.AttachedSideConfig;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.network.MekClickType;
import mekanism.common.network.to_server.PacketGuiInteract;
import mekanism.common.network.to_server.PacketGuiInteract.GuiInteraction;
import mekanism.common.network.to_server.configuration_update.PacketBatchConfiguration;
import mekanism.common.network.to_server.configuration_update.PacketEjectColor;
import mekanism.common.network.to_server.configuration_update.PacketEjectConfiguration;
import mekanism.common.network.to_server.configuration_update.PacketInputColor;
import mekanism.common.network.to_server.configuration_update.PacketSideData;
import net.neoforged.neoforge.network.handling.IPayloadContext;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismDataComponents;
import mekanism.common.registries.MekanismItems;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.component.TileComponentEjector;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.tile.interfaces.IRedstoneControl.RedstoneControl;
import mekanism.common.tile.prefab.TileEntityConfigurableMachine;
import mekanism.generators.common.registries.GeneratorsBlocks;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponentMap;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.Container;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.world.level.storage.ValueInput;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.transfer.ResourceHandler;
import net.neoforged.neoforge.transfer.energy.EnergyHandler;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Frente `config-lados`: grava como a configuração de lados (`TileComponentConfig`, `TileComponentEjector`, `ConfigHolder`) das quatro máquinas portadas
/// e o que o Heat Generator expõe por face. Um `runGameTestServer`, os arquivos `config_lados_*.jsonl`:
/// `caps` (as capabilities de item, energia e químico por face, máquina posta por `setBlock` e por um jogador, cada modo posto, a máquina girada, modos que a
/// máquina não suporta), `increment` (a sequência do botão de modo), `nbt` (o tile salvo, o item, o tile relido, o item que cai), `card` (cartão de
/// configuração) e `eject_<cenário>` (tick a tick, com um baú em cada face).
@ForEachTest(groups = "trace")
public class ConfigLadosTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final String COMMIT = "b2d04e166";
    private static final String DATE = "2026-11-09";
    private static final BlockPos P1 = new BlockPos(1, 1, 1);
    private static final BlockPos P2 = new BlockPos(3, 1, 1);
    private static final List<Direction> HORIZONTAL = List.of(Direction.NORTH, Direction.EAST, Direction.SOUTH, Direction.WEST);
    private static final int EJECT_TICKS = 80;

    private record Spec(String name, Supplier<ItemLike> item, Supplier<BlockState> state, boolean configurable, int input, int output, int infusion) {
    }

    private static final Spec CHAMBER = new Spec("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem(),
          () -> MekanismBlocks.ENRICHMENT_CHAMBER.defaultState(), true, 0, 1, -1);
    private static final Spec CRUSHER = new Spec("crusher", () -> MekanismBlocks.CRUSHER.asItem(),
          () -> MekanismBlocks.CRUSHER.defaultState(), true, 0, 1, -1);
    private static final Spec SMELTER = new Spec("energized_smelter", () -> MekanismBlocks.ENERGIZED_SMELTER.asItem(),
          () -> MekanismBlocks.ENERGIZED_SMELTER.defaultState(), true, 0, 1, -1);
    private static final Spec INFUSER = new Spec("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.asItem(),
          () -> MekanismBlocks.METALLURGIC_INFUSER.defaultState(), true, 1, 2, 0);
    private static final Spec HEAT = new Spec("heat_generator", () -> GeneratorsBlocks.HEAT_GENERATOR.asItem(),
          () -> GeneratorsBlocks.HEAT_GENERATOR.defaultState(), false, -1, -1, -1);
    private static final List<Spec> MACHINES = List.of(CHAMBER, CRUSHER, SMELTER, INFUSER);
    private static final List<Spec> ALL = List.of(CHAMBER, CRUSHER, SMELTER, INFUSER, HEAT);

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(5, 3, 3).fill(0, 0, 1, 4, 0, 1, Blocks.STONE.defaultBlockState()));
    }

    private static void smallTemplate(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(1, 0, 1, 1, 0, 1, Blocks.STONE.defaultBlockState()));
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Os testes
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_config_lados")
    @TestHolder(description = "Records the item, energy and chemical capabilities per face: placed by setBlock and by a player, every mode set, rotated, unsupported modes.")
    public static void caps(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> capsAll(new Env(helper)))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_config_lados")
    @TestHolder(description = "Records the sequence of data types of incrementDataType and decrementDataType for every machine, type and side, and addDisabledSides.")
    public static void increment(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> incrementAll(new Env(helper)))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_config_lados")
    @TestHolder(description = "Records the saved tile NBT, the item components, the reloaded tile, the placed item and the dropped item for several side configurations.")
    public static void nbt(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> nbtAll(new Env(helper)))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_config_lados")
    @TestHolder(description = "Records the configuration card: writeConfigurationData, setConfigurationData with partial data, and isConfigurationDataCompatible.")
    public static void card(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> cardAll(new Env(helper)))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_config_lados")
    @TestHolder(description = "Records the Enrichment Chamber ejecting from its right side, with ejecting off in the first 20 ticks.")
    public static void ejectOutputRight(final DynamicTest test) {
        smallTemplate(test);
        Eject scene = new Eject("output_right", CHAMBER, "OUTPUT on the right (the item default), ejecting off in the first 20 ticks and on from tick 20. The output slot is refilled to 64 clay balls every 20 ticks.");
        scene.itemEjecting(0, false);
        scene.refillOutput(0, 20, 40, 60);
        scene.itemEjecting(20, true);
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_config_lados")
    @TestHolder(description = "Records the Enrichment Chamber ejecting from the right, the back and the bottom.")
    public static void ejectOutputThree(final DynamicTest test) {
        smallTemplate(test);
        Eject scene = new Eject("output_three", CHAMBER, "OUTPUT on the right, the back and the bottom, ejecting on from tick 0. The output slot is refilled to 64 clay balls every 20 ticks.");
        scene.setItem(0, RelativeSide.RIGHT, DataType.OUTPUT);
        scene.setItem(0, RelativeSide.BACK, DataType.OUTPUT);
        scene.setItem(0, RelativeSide.BOTTOM, DataType.OUTPUT);
        scene.itemEjecting(0, true);
        scene.refillOutput(0, 20, 40, 60);
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_config_lados")
    @TestHolder(description = "Records the Enrichment Chamber with INPUT_OUTPUT on the front and an item in the input slot besides the output slot.")
    public static void ejectInputOutputFront(final DynamicTest test) {
        smallTemplate(test);
        Eject scene = new Eject("input_output_front", CHAMBER, "INPUT_OUTPUT on the front, the right side changed to INPUT so only the front can output. 5 clay in the input slot and 64 clay balls in the output slot, both refilled every 20 ticks. No energy: the recipe never starts.");
        scene.setItem(0, RelativeSide.FRONT, DataType.INPUT_OUTPUT);
        scene.setItem(0, RelativeSide.RIGHT, DataType.INPUT);
        scene.itemEjecting(0, true);
        scene.refillOutput(0, 20, 40, 60);
        scene.refillInput(0, 20, 40, 60);
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_config_lados")
    @TestHolder(description = "Records the Enrichment Chamber ejecting with every item side NONE.")
    public static void ejectNoData(final DynamicTest test) {
        smallTemplate(test);
        Eject scene = new Eject("no_data", CHAMBER, "Every item side NONE and ejecting on from tick 0. The output slot is refilled to 64 clay balls every 20 ticks.");
        for (RelativeSide side : RelativeSide.VALUES) {
            scene.setItem(0, side, DataType.NONE);
        }
        scene.itemEjecting(0, true);
        scene.refillOutput(0, 20, 40, 60);
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_config_lados")
    @TestHolder(description = "Records the Enrichment Chamber rotated to another front while ejecting from its right side.")
    public static void ejectRotate(final DynamicTest test) {
        smallTemplate(test);
        Eject scene = new Eject("rotate", CHAMBER, "OUTPUT on the right (north front: west chest), ejecting on from tick 0. At tick 40 TileEntityMekanism.setFacing(EAST), the path of the wrench. The output slot is refilled to 64 clay balls every 20 ticks.");
        scene.itemEjecting(0, true);
        scene.refillOutput(0, 20, 40, 60);
        scene.at(40, "setFacing(EAST)", () -> scene.machine().setFacing(Direction.EAST));
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_config_lados")
    @TestHolder(description = "Records the Metallurgic Infuser with EXTRA on the bottom and an item in the infusion slot, then OUTPUT on the bottom.")
    public static void ejectInfuserExtra(final DynamicTest test) {
        smallTemplate(test);
        Eject scene = new Eject("infuser_extra", INFUSER, "BOTTOM=EXTRA (the item default) with 3 sticks in the infusion slot (a stick is not convertible, so it stays) and 64 clay balls in the output slot, ejecting on from tick 0, the right side changed to INPUT so the output slot can only leave through the bottom. At tick 40 BOTTOM=OUTPUT.");
        scene.setItem(0, RelativeSide.RIGHT, DataType.INPUT);
        scene.setItem(0, RelativeSide.BOTTOM, DataType.EXTRA);
        scene.itemEjecting(0, true);
        scene.at(0, "infusion slot 3 stick", () -> scene.setSlot(scene.spec.infusion, Items.STICK, 3));
        scene.refillOutput(0, 20, 40, 60);
        scene.setItem(40, RelativeSide.BOTTOM, DataType.OUTPUT);
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_config_lados")
    @TestHolder(description = "Records the Enrichment Chamber loaded from NBT with ejecting on only for ENERGY.")
    public static void ejectStrictOtherType(final DynamicTest test) {
        smallTemplate(test);
        Eject scene = new Eject("strict_other_type", CHAMBER, "The tile is saved, eject0 (ENERGY) is set to 1 and eject3 (ITEM) to 0 in component_config, and the tile is read back by BlockEntity.loadStatic and attached to the world: ejecting only for ENERGY, OUTPUT on the right. At tick 40 the item ejecting is turned on (positive control). The output slot is refilled to 64 clay balls every 20 ticks.");
        scene.viaNbt = true;
        scene.refillOutput(0, 20, 40, 60);
        scene.itemEjecting(40, true);
        run(test, scene);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_config_lados")
    @TestHolder(description = "Records the state after each packet of the side configuration and transporter windows: PacketSideData, PacketEjectConfiguration, PacketBatchConfiguration, PacketEjectColor, PacketInputColor and STRICT_INPUT.")
    public static void packets(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> packetsAll(new Env(helper)))
              .thenSucceed());
    }

    private static IPayloadContext context(Player who) {
        return (IPayloadContext) java.lang.reflect.Proxy.newProxyInstance(IPayloadContext.class.getClassLoader(), new Class<?>[]{IPayloadContext.class},
              (proxy, method, args) -> {
                  if (method.getName().equals("player")) {
                      return who;
                  }
                  throw new UnsupportedOperationException(method.getName());
              });
    }

    /// Cada pacote que as janelas mandam, tratado pelo `handle` do original sobre uma máquina posta por um jogador: a linha diz o pacote e o estado depois dele.
    private static void packetsAll(Env e) {
        Out out = new Out("config_pacotes", "State after each configuration packet", "machine, step, packet, config, ejecting, ejector");
        for (Spec spec : MACHINES) {
            TileEntityMekanism placed = e.useOn(spec, P1, Direction.EAST);
            TileEntityConfigurableMachine machine = (TileEntityConfigurableMachine) placed;
            BlockPos pos = e.abs(P1);
            IPayloadContext context = context(e.player);
            int[] step = {0};
            java.util.function.BiConsumer<JsonObject, Runnable> record = (packet, send) -> {
                send.run();
                JsonObject line = new JsonObject();
                line.addProperty("machine", spec.name);
                line.addProperty("step", step[0]++);
                line.add("packet", packet);
                line.add("config", configJson(machine));
                line.add("ejecting", ejectingJson(machine));
                line.add("ejector", ejectorJson(machine));
                out.add(line);
            };
            List<TransmissionType> types = machine.getConfig().getTransmissions();
            MekClickType[] clicks = {MekClickType.LEFT, MekClickType.LEFT, MekClickType.LEFT, MekClickType.LEFT, MekClickType.LEFT, MekClickType.RIGHT, MekClickType.RIGHT,
                                     MekClickType.SHIFT_LEFT, MekClickType.SHIFT_LEFT, MekClickType.RIGHT, MekClickType.RIGHT, MekClickType.LEFT};
            for (TransmissionType type : types) {
                for (RelativeSide side : RelativeSide.VALUES) {
                    for (MekClickType click : clicks) {
                        JsonObject packet = new JsonObject();
                        packet.addProperty("kind", "side_data");
                        packet.addProperty("click", click.name());
                        packet.addProperty("side", side.getSerializedName());
                        packet.addProperty("transmission", type.getSerializedName());
                        record.accept(packet, () -> new PacketSideData(pos, click, side, type).handle(context));
                    }
                }
                for (int i = 0; i < 2; i++) {
                    JsonObject packet = new JsonObject();
                    packet.addProperty("kind", "eject");
                    packet.addProperty("transmission", type.getSerializedName());
                    record.accept(packet, () -> new PacketEjectConfiguration(pos, type).handle(context));
                }
                for (DataType target : DataType.VALUES) {
                    JsonObject packet = new JsonObject();
                    packet.addProperty("kind", "batch");
                    packet.addProperty("transmission", type.getSerializedName());
                    packet.addProperty("target", target.getSerializedName());
                    record.accept(packet, () -> new PacketBatchConfiguration(pos, type, target).handle(context));
                }
            }
            for (DataType target : new DataType[]{DataType.INPUT, DataType.NONE, DataType.OUTPUT}) {
                JsonObject packet = new JsonObject();
                packet.addProperty("kind", "batch");
                packet.addProperty("transmission", "");
                packet.addProperty("target", target.getSerializedName());
                record.accept(packet, () -> new PacketBatchConfiguration(pos, null, target).handle(context));
            }
            MekClickType[] colorClicks = {MekClickType.RIGHT, MekClickType.LEFT, MekClickType.LEFT, MekClickType.SHIFT_LEFT, MekClickType.LEFT, MekClickType.RIGHT, MekClickType.RIGHT};
            for (int i = 0; i < 20; i++) {
                JsonObject packet = new JsonObject();
                packet.addProperty("kind", "eject_color");
                packet.addProperty("click", MekClickType.LEFT.name());
                record.accept(packet, () -> new PacketEjectColor(pos, MekClickType.LEFT).handle(context));
            }
            for (MekClickType click : colorClicks) {
                JsonObject packet = new JsonObject();
                packet.addProperty("kind", "eject_color");
                packet.addProperty("click", click.name());
                record.accept(packet, () -> new PacketEjectColor(pos, click).handle(context));
            }
            for (RelativeSide side : RelativeSide.VALUES) {
                for (int i = 0; i < 20; i++) {
                    JsonObject packet = new JsonObject();
                    packet.addProperty("kind", "input_color");
                    packet.addProperty("click", MekClickType.LEFT.name());
                    packet.addProperty("side", side.getSerializedName());
                    record.accept(packet, () -> new PacketInputColor(pos, MekClickType.LEFT, side).handle(context));
                }
                for (MekClickType click : colorClicks) {
                    JsonObject packet = new JsonObject();
                    packet.addProperty("kind", "input_color");
                    packet.addProperty("click", click.name());
                    packet.addProperty("side", side.getSerializedName());
                    record.accept(packet, () -> new PacketInputColor(pos, click, side).handle(context));
                }
            }
            for (int i = 0; i < 2; i++) {
                JsonObject packet = new JsonObject();
                packet.addProperty("kind", "strict_input");
                record.accept(packet, () -> new PacketGuiInteract(GuiInteraction.STRICT_INPUT, machine).handle(context));
            }
        }
        out.write();
    }

    private static void run(DynamicTest test, Eject scene) {
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> scene.pre());
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.post());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper))
              .thenExecuteAfter(EJECT_TICKS + 20, scene::finish)
              .thenSucceed());
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Escrita
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final class Out {

        private final String name;
        private final List<String> lines = new ArrayList<>();

        Out(String name, String description, String fields) {
            this.name = name;
            JsonObject meta = new JsonObject();
            meta.addProperty("commit", COMMIT);
            meta.addProperty("date", DATE);
            meta.addProperty("file", name + ".jsonl");
            meta.addProperty("description", description);
            meta.addProperty("fields", fields);
            JsonObject line = new JsonObject();
            line.add("meta", meta);
            lines.add(line.toString());
        }

        void add(JsonObject line) {
            lines.add(line.toString());
        }

        void write() {
            try {
                Path file = Path.of("trace", name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Ambiente: a máquina posta de dois jeitos
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final class Env {

        final MekGameTestHelper helper;
        final Player player;

        Env(MekGameTestHelper helper) {
            this.helper = helper;
            this.player = helper.makeMockPlayerLookingAt(P1.below(), Direction.UP);
        }

        ServerLevel level() {
            return helper.getLevel();
        }

        BlockPos abs(BlockPos rel) {
            return helper.absolutePos(rel);
        }

        TileEntityMekanism tile(BlockPos rel) {
            return (TileEntityMekanism) level().getBlockEntity(abs(rel));
        }

        void clear(BlockPos rel) {
            helper.setBlock(rel, Blocks.AIR.defaultBlockState());
        }

        /// Posta pelo gabarito: `setBlock`, que nunca passa por `applyImplicitComponents`.
        TileEntityMekanism setBlock(Spec spec, BlockPos rel, Direction facing) {
            clear(rel);
            BlockState state = Attribute.setFacing(spec.state.get(), facing);
            helper.setBlock(rel, state);
            return prepare(tile(rel));
        }

        /// Posta por um jogador, `useOn` com o item (que traz a configuração padrão), com o jogador virado para que a máquina saia com `facing`.
        TileEntityMekanism useOn(Spec spec, BlockPos rel, Direction facing, ItemStack stack) {
            clear(rel);
            player.setYRot(facing.getOpposite().toYRot());
            helper.useOn(rel.below(), stack, player, Direction.UP);
            TileEntityMekanism tile = tile(rel);
            if (tile == null || tile.getDirection() != facing) {
                throw new IllegalStateException("placement by player gave " + (tile == null ? null : tile.getDirection()) + " instead of " + facing);
            }
            return prepare(tile);
        }

        TileEntityMekanism useOn(Spec spec, BlockPos rel, Direction facing) {
            return useOn(spec, rel, facing, new ItemStack(spec.item.get()));
        }

        /// Capacidade grande e vazio: o que o `insert` simulado de energia precisa.
        TileEntityMekanism prepare(TileEntityMekanism tile) {
            if (tile.getEnergyContainer() instanceof MachineEnergyContainer<?> energy) {
                energy.setMaxEnergy(1_000_000);
                energy.setEnergy(0, null);
            }
            return tile;
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Leitura do estado
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static Object field(Object owner, Class<?> declaring, String name) {
        try {
            Field field = declaring.getDeclaredField(name);
            field.setAccessible(true);
            return field.get(owner);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    private static IHolder holderOf(Object manager) {
        return manager == null ? null : (IHolder) field(manager, BasicSidedCapabilityResolver.class, "holder");
    }

    private static JsonArray indices(List<?> all, List<?> sub) {
        JsonArray array = new JsonArray();
        for (Object element : sub) {
            int index = -1;
            for (int i = 0; i < all.size(); i++) {
                if (all.get(i) == element) {
                    index = i;
                }
            }
            array.add(index);
        }
        return array;
    }

    private static String name(DataType type) {
        return type.getSerializedName();
    }

    private static JsonArray strings(Iterable<String> values) {
        JsonArray array = new JsonArray();
        for (String value : values) {
            array.add(value);
        }
        return array;
    }

    /// As capabilities de item, energia e químico de cada face, como quem está do lado de fora as vê (`Level.getCapability`), e as listas de slots e tanques
    /// que o `ConfigHolder` devolve para a face.
    @SuppressWarnings("unchecked")
    private static void capsInto(Env e, TileEntityMekanism tile, BlockPos rel, JsonObject out) {
        ServerLevel level = e.level();
        BlockPos abs = e.abs(rel);
        Object itemManager = field(tile, TileEntityMekanism.class, "itemHandlerManager");
        Object energyManager = field(tile, TileEntityMekanism.class, "energyHandlerManager");
        Object chemicalManager = field(tile, TileEntityMekanism.class, "chemicalHandlerManager");
        JsonObject item = new JsonObject();
        JsonObject energy = new JsonObject();
        JsonObject chemical = new JsonObject();
        for (Direction face : Direction.values()) {
            String key = face.getSerializedName();
            ResourceHandler<?> itemHandler = level.getCapability(Capabilities.ITEM.block(), abs, face);
            JsonObject i = new JsonObject();
            i.addProperty("present", itemHandler != null);
            if (itemManager != null) {
                i.add("slots", indices(tile.getInventorySlots(), ((ResourceHandlerManager<?, ?>) itemManager).getContainers(face)));
                IHolder holder = holderOf(itemManager);
                i.addProperty("can_insert", holder.canInsert(face));
                i.addProperty("can_extract", holder.canExtract(face));
            }
            item.add(key, i);
            EnergyHandler energyHandler = level.getCapability(Capabilities.ENERGY.block(), abs, face);
            JsonObject en = new JsonObject();
            en.addProperty("present", energyHandler != null);
            int accepted = 0;
            if (energyHandler != null) {
                try (Transaction transaction = Transaction.openRoot()) {
                    accepted = energyHandler.insert(1000, transaction);
                }
            }
            en.addProperty("accepted", accepted);
            if (energyManager != null) {
                IHolder holder = holderOf(energyManager);
                en.addProperty("can_insert", holder.canInsert(face));
                en.addProperty("can_extract", holder.canExtract(face));
            }
            energy.add(key, en);
            if (chemicalManager != null) {
                ResourceHandler<?> chemicalHandler = level.getCapability(Capabilities.CHEMICAL.block(), abs, face);
                JsonObject c = new JsonObject();
                c.addProperty("present", chemicalHandler != null);
                c.add("slots", indices(tile.getChemicalTanks(), ((ResourceHandlerManager<?, ?>) chemicalManager).getContainers(face)));
                IHolder holder = holderOf(chemicalManager);
                c.addProperty("can_insert", holder.canInsert(face));
                c.addProperty("can_extract", holder.canExtract(face));
                chemical.add(key, c);
            }
        }
        out.add("item", item);
        out.add("energy", energy);
        if (chemicalManager != null) {
            out.add("chemical", chemical);
        }
    }

    private static JsonObject configJson(TileEntityConfigurableMachine machine) {
        JsonObject config = new JsonObject();
        for (TransmissionType type : machine.getConfig().getTransmissions()) {
            JsonArray sides = new JsonArray();
            ConfigInfo info = machine.getConfig().getConfig(type);
            for (RelativeSide side : RelativeSide.VALUES) {
                sides.add(name(info.getDataType(side)));
            }
            config.add(type.getSerializedName(), sides);
        }
        return config;
    }

    private static JsonObject ejectingJson(TileEntityConfigurableMachine machine) {
        JsonObject ejecting = new JsonObject();
        for (TransmissionType type : machine.getConfig().getTransmissions()) {
            ejecting.addProperty(type.getSerializedName(), machine.getConfig().getConfig(type).isEjecting());
        }
        return ejecting;
    }

    private static JsonObject canEjectJson(TileEntityConfigurableMachine machine) {
        JsonObject canEject = new JsonObject();
        for (TransmissionType type : machine.getConfig().getTransmissions()) {
            canEject.addProperty(type.getSerializedName(), machine.getConfig().getConfig(type).canEject());
        }
        return canEject;
    }

    private static JsonObject supportedJson(TileEntityConfigurableMachine machine) {
        JsonObject supported = new JsonObject();
        for (TransmissionType type : machine.getConfig().getTransmissions()) {
            JsonArray array = new JsonArray();
            for (DataType dataType : machine.getConfig().getConfig(type).getSupportedDataTypes()) {
                array.add(name(dataType));
            }
            supported.add(type.getSerializedName(), array);
        }
        return supported;
    }

    private static JsonObject ejectorJson(TileEntityConfigurableMachine machine) {
        TileComponentEjector ejector = machine.getEjector();
        JsonObject out = new JsonObject();
        out.addProperty("strict_input", ejector.hasStrictInput());
        out.addProperty("output_color", ejector.getOutputColor() == null ? "" : ejector.getOutputColor().getSerializedName());
        JsonArray inputs = new JsonArray();
        for (RelativeSide side : RelativeSide.VALUES) {
            EnumColor color = ejector.getInputColor(side);
            inputs.add(color == null ? "" : color.getSerializedName());
        }
        out.add("input_colors", inputs);
        return out;
    }

    /// Lados, ejetar, ejetor e modo de redstone.
    private static JsonObject stateJson(TileEntityMekanism tile) {
        JsonObject out = new JsonObject();
        out.addProperty("facing", tile.getDirection().getSerializedName());
        out.addProperty("control_type", tile.getControlType().getSerializedName());
        if (tile instanceof TileEntityConfigurableMachine machine) {
            out.add("config", configJson(machine));
            out.add("ejecting", ejectingJson(machine));
            out.add("ejector", ejectorJson(machine));
        }
        return out;
    }

    private static JsonObject line(Env e, Spec spec, String origin, TileEntityMekanism tile, String step, BlockPos rel) {
        JsonObject out = new JsonObject();
        out.addProperty("machine", spec.name);
        out.addProperty("origin", origin);
        out.addProperty("facing", tile.getDirection().getSerializedName());
        out.addProperty("step", step);
        if (tile instanceof TileEntityConfigurableMachine machine) {
            out.add("config", configJson(machine));
            out.add("ejecting", ejectingJson(machine));
            out.add("can_eject", canEjectJson(machine));
            out.add("supported", supportedJson(machine));
        }
        capsInto(e, tile, rel, out);
        return out;
    }

    private static TileEntityConfigurableMachine machine(TileEntityMekanism tile) {
        return (TileEntityConfigurableMachine) tile;
    }

    @SuppressWarnings("unchecked")
    private static JsonElement encode(Env e, Codec<?> codec, Object value) {
        return encodeTyped(e, (Codec<Object>) codec, value);
    }

    private static <T> JsonElement encodeTyped(Env e, Codec<T> codec, T value) {
        if (value == null) {
            return com.google.gson.JsonNull.INSTANCE;
        }
        return codec.encodeStart(e.level().registryAccess().createSerializationContext(JsonOps.INSTANCE), value).getOrThrow(IllegalStateException::new);
    }

    private static JsonElement sorted(JsonElement element) {
        if (element.isJsonObject()) {
            JsonObject in = element.getAsJsonObject();
            List<String> keys = new ArrayList<>(in.keySet());
            Collections.sort(keys);
            JsonObject out = new JsonObject();
            for (String key : keys) {
                out.add(key, sorted(in.get(key)));
            }
            return out;
        } else if (element.isJsonArray()) {
            JsonArray out = new JsonArray();
            for (JsonElement child : element.getAsJsonArray()) {
                out.add(sorted(child));
            }
            return out;
        }
        return element;
    }

    private static JsonElement nbtJson(Tag tag) {
        return tag == null ? com.google.gson.JsonNull.INSTANCE : sorted(NbtOps.INSTANCE.convertTo(JsonOps.INSTANCE, tag));
    }

    private static JsonObject nbtSub(CompoundTag tag, String key) {
        JsonObject out = new JsonObject();
        Tag child = tag.get(key);
        out.addProperty("present", child != null);
        out.add("json", nbtJson(child));
        out.addProperty("snbt", child == null ? "" : child.toString());
        return out;
    }

    private static JsonArray patchKeys(ItemStack stack) {
        List<String> keys = new ArrayList<>();
        DataComponentPatch.SplitResult split = stack.getComponentsPatch().split();
        for (DataComponentType<?> type : split.added().keySet()) {
            keys.add(String.valueOf(BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(type)));
        }
        for (DataComponentType<?> type : split.removed()) {
            keys.add("!" + BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(type));
        }
        Collections.sort(keys);
        return strings(keys);
    }

    /// A pilha sem o dono: o jogador que posta a máquina deixa `mekanism:owner` no item que cai, e só isso já impede de empilhar.
    private static ItemStack withoutOwner(ItemStack stack) {
        ItemStack copy = stack.copy();
        copy.remove(MekanismDataComponents.OWNER.get());
        return copy;
    }

    private static JsonObject itemJson(Env e, ItemStack stack) {
        JsonObject out = new JsonObject();
        out.addProperty("item", BuiltInRegistries.ITEM.getKey(stack.getItem()).toString());
        out.addProperty("count", stack.getCount());
        out.add("side_config", encode(e, AttachedSideConfig.CODEC, stack.get(MekanismDataComponents.SIDE_CONFIG)));
        out.add("ejector", encode(e, AttachedEjector.CODEC, stack.get(MekanismDataComponents.EJECTOR)));
        out.add("patch", patchKeys(stack));
        return out;
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Configurações postas à mão
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static void setMode(TileEntityConfigurableMachine machine, TransmissionType type, RelativeSide side, DataType dataType) {
        machine.getConfig().getConfig(type).setDataType(dataType, side);
        machine.getConfig().sideChanged(type, side);
    }

    /// Lados mudados à mão: em cada tipo, o lado `i` recebe o DataType suportado de índice `(i + 1 + ordinal do tipo)` (volta ao começo), o que dá um padrão sem
    /// repetir o padrão do item.
    private static void patternA(TileEntityConfigurableMachine machine) {
        for (TransmissionType type : machine.getConfig().getTransmissions()) {
            List<DataType> supported = new ArrayList<>(machine.getConfig().getConfig(type).getSupportedDataTypes());
            for (int i = 0; i < RelativeSide.VALUES.size(); i++) {
                setMode(machine, type, RelativeSide.VALUES.get(i), supported.get((i + 1 + type.ordinal()) % supported.size()));
            }
        }
    }

    private static void ejectorA(TileEntityConfigurableMachine machine) {
        machine.getEjector().setStrictInput(true);
        machine.getEjector().setOutputColor(EnumColor.DARK_GREEN);
        machine.getEjector().setInputColor(RelativeSide.LEFT, EnumColor.RED);
        machine.getEjector().setInputColor(RelativeSide.BOTTOM, EnumColor.DARK_BLUE);
    }

    private static void ejectorB(TileEntityConfigurableMachine machine) {
        machine.getEjector().setStrictInput(false);
        machine.getEjector().setOutputColor(EnumColor.YELLOW);
        machine.getEjector().setInputColor(RelativeSide.TOP, EnumColor.PINK);
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Arquivo 1: config_lados_caps
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static void capsAll(Env e) {
        Out out = new Out("config_lados_caps",
              "Capabilities per face of the four machines (and the Heat Generator) under every configuration the Java can reach.",
              "machine; origin (set_block: setBlock, no applyImplicitComponents; item: a player with useOn and the plain item); facing; step; config {type: [DataType x6 in RelativeSide.VALUES order: front,left,right,back,top,bottom]}; "
                    + "ejecting {type: bool}; can_eject {type: bool}; supported {type: [DataType in getSupportedDataTypes order]}; "
                    + "item/energy/chemical {world face: ...}. item.present: Level.getCapability(ITEM.block()) is not null (null when isCapabilityDisabled); item.slots: indices into getInventorySlots(null) of the slots "
                    + "the ConfigHolder returns for the face (getContainers(face)), regardless of present; can_insert/can_extract: ConfigHolder.canInsert/canExtract(face). "
                    + "energy.accepted: a simulated EnergyHandler.insert(1000) on the face with the container emptied and the max energy raised to 1000000 (0 when absent). chemical (only the infuser): the same present/slots/can_*, from the tank holder. "
                    + "set_mode lines: the DataType put through ConfigInfo.setDataType plus TileComponentConfig.sideChanged, then reset to the default; set_mode_unsupported lines: ConfigInfo.setDataType directly (no support check), caps_before_side_changed is the reading right after setDataType and before sideChanged (after a first read to fill the caches), the main fields are after sideChanged.");
        // (a) placed
        for (Spec spec : ALL) {
            for (String origin : List.of("set_block", "item")) {
                for (Direction facing : HORIZONTAL) {
                    TileEntityMekanism tile = origin.equals("item") ? e.useOn(spec, P1, facing) : e.setBlock(spec, P1, facing);
                    out.add(line(e, spec, origin, tile, "placed", P1));
                }
            }
        }
        // (b) cada modo
        for (Spec spec : MACHINES) {
            for (Direction facing : List.of(Direction.NORTH, Direction.EAST)) {
                TileEntityConfigurableMachine machine = machine(e.useOn(spec, P1, facing));
                for (TransmissionType type : machine.getConfig().getTransmissions()) {
                    ConfigInfo info = machine.getConfig().getConfig(type);
                    for (RelativeSide side : RelativeSide.VALUES) {
                        DataType previous = info.getDataType(side);
                        for (DataType dataType : new ArrayList<>(info.getSupportedDataTypes())) {
                            setMode(machine, type, side, dataType);
                            JsonObject line = line(e, spec, "item", machine, "set_mode:" + type.getSerializedName() + ":" + side.getSerializedName() + ":" + name(dataType), P1);
                            line.addProperty("same_as_default", dataType == previous);
                            out.add(line);
                            setMode(machine, type, side, previous);
                        }
                    }
                }
            }
        }
        // (c) girada
        for (Spec spec : MACHINES) {
            for (String path : List.of("set_facing", "set_block_state")) {
                TileEntityConfigurableMachine machine = machine(e.useOn(spec, P1, Direction.NORTH));
                for (TransmissionType type : machine.getConfig().getTransmissions()) {
                    ConfigInfo info = machine.getConfig().getConfig(type);
                    for (Object[] pair : new Object[][]{{RelativeSide.RIGHT, DataType.OUTPUT}, {RelativeSide.FRONT, DataType.INPUT_OUTPUT}, {RelativeSide.LEFT, DataType.EXTRA}}) {
                        if (info.supports((DataType) pair[1])) {
                            setMode(machine, type, (RelativeSide) pair[0], (DataType) pair[1]);
                        }
                    }
                }
                for (Direction facing : HORIZONTAL) {
                    if (machine.getDirection() != facing) {
                        if (path.equals("set_facing")) {
                            machine.setFacing(facing);
                        } else {
                            e.helper.setBlock(P1, Attribute.setFacing(e.level().getBlockState(e.abs(P1)), facing));
                        }
                    }
                    JsonObject line = line(e, spec, "item", machine, "rotated:" + path + ":" + facing.getSerializedName(), P1);
                    line.addProperty("same_tile", e.tile(P1) == machine);
                    line.addProperty("state_facing", Attribute.getFacing(e.level().getBlockState(e.abs(P1))).getSerializedName());
                    out.add(line);
                }
            }
        }
        // (d) DataType que o tipo não suporta, posto direto
        for (Spec spec : MACHINES) {
            TileEntityConfigurableMachine machine = machine(e.useOn(spec, P1, Direction.NORTH));
            for (TransmissionType type : machine.getConfig().getTransmissions()) {
                ConfigInfo info = machine.getConfig().getConfig(type);
                for (DataType dataType : DataType.VALUES) {
                    if (info.supports(dataType)) {
                        continue;
                    }
                    for (RelativeSide side : RelativeSide.VALUES) {
                        DataType previous = info.getDataType(side);
                        JsonObject discard = new JsonObject();
                        capsInto(e, machine, P1, discard);
                        info.setDataType(dataType, side);
                        JsonObject before = new JsonObject();
                        capsInto(e, machine, P1, before);
                        machine.getConfig().sideChanged(type, side);
                        JsonObject line = line(e, spec, "item", machine, "set_mode_unsupported:" + type.getSerializedName() + ":" + side.getSerializedName() + ":" + name(dataType), P1);
                        line.addProperty("slot_info_null", info.getSlotInfo(side) == null);
                        line.add("caps_before_side_changed", before);
                        out.add(line);
                        setMode(machine, type, side, previous);
                    }
                }
            }
        }
        out.write();
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Arquivo 2: config_lados_increment
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static void incrementAll(Env e) {
        Out out = new Out("config_lados_increment",
              "ConfigInfo.incrementDataType and decrementDataType from the item defaults, plus addDisabledSides.",
              "machine; type; side; start (the DataType before); supported (getSupportedDataTypes order); increment/decrement: the 12 DataTypes returned by 12 calls in a row (each call stores the result in the side), the side is reset to start between the two. "
                    + "disabled_sides line: the machine placed by a player, addDisabledSides(TOP) called on the TileComponentConfig, and what that does to config, setDataType (its return value), incrementDataType, the ejector input color and the capabilities. "
                    + "None of the four machines calls addDisabledSides (only the Isotopic Centrifuge, the Solar Neutron Activator and the Pigment Mixer do).");
        for (Spec spec : MACHINES) {
            TileEntityConfigurableMachine machine = machine(e.useOn(spec, P1, Direction.NORTH));
            for (TransmissionType type : machine.getConfig().getTransmissions()) {
                ConfigInfo info = machine.getConfig().getConfig(type);
                for (RelativeSide side : RelativeSide.VALUES) {
                    DataType start = info.getDataType(side);
                    JsonObject line = new JsonObject();
                    line.addProperty("machine", spec.name);
                    line.addProperty("type", type.getSerializedName());
                    line.addProperty("side", side.getSerializedName());
                    line.addProperty("start", name(start));
                    JsonArray supported = new JsonArray();
                    for (DataType dataType : info.getSupportedDataTypes()) {
                        supported.add(name(dataType));
                    }
                    line.add("supported", supported);
                    JsonArray increment = new JsonArray();
                    for (int i = 0; i < 12; i++) {
                        increment.add(name(info.incrementDataType(side)));
                    }
                    info.setDataType(start, side);
                    JsonArray decrement = new JsonArray();
                    for (int i = 0; i < 12; i++) {
                        decrement.add(name(info.decrementDataType(side)));
                    }
                    info.setDataType(start, side);
                    line.add("increment", increment);
                    line.add("decrement", decrement);
                    out.add(line);
                }
            }
        }
        for (Spec spec : MACHINES) {
            TileEntityConfigurableMachine machine = machine(e.useOn(spec, P1, Direction.NORTH));
            JsonObject line = new JsonObject();
            line.addProperty("machine", spec.name);
            line.addProperty("step", "disabled_sides");
            line.add("config_before", configJson(machine));
            machine.getConfig().addDisabledSides(RelativeSide.TOP);
            line.add("config_after", configJson(machine));
            JsonObject results = new JsonObject();
            for (TransmissionType type : machine.getConfig().getTransmissions()) {
                ConfigInfo info = machine.getConfig().getConfig(type);
                JsonObject result = new JsonObject();
                result.addProperty("is_side_enabled_top", info.isSideEnabled(RelativeSide.TOP));
                result.addProperty("is_side_enabled_front", info.isSideEnabled(RelativeSide.FRONT));
                List<DataType> supported = new ArrayList<>(info.getSupportedDataTypes());
                result.addProperty("set_data_type_top_returns", info.setDataType(supported.get(supported.size() - 1), RelativeSide.TOP));
                result.addProperty("increment_top", name(info.incrementDataType(RelativeSide.TOP)));
                result.addProperty("decrement_top", name(info.decrementDataType(RelativeSide.TOP)));
                results.add(type.getSerializedName(), result);
            }
            line.add("results", results);
            machine.getEjector().setInputColor(RelativeSide.TOP, EnumColor.RED);
            machine.getEjector().setInputColor(RelativeSide.FRONT, EnumColor.RED);
            line.addProperty("input_side_enabled_top", machine.getEjector().isInputSideEnabled(RelativeSide.TOP));
            line.addProperty("input_color_top_after_set", machine.getEjector().getInputColor(RelativeSide.TOP) == null ? "" : machine.getEjector().getInputColor(RelativeSide.TOP).getSerializedName());
            line.addProperty("input_color_front_after_set", machine.getEjector().getInputColor(RelativeSide.FRONT) == null ? "" : machine.getEjector().getInputColor(RelativeSide.FRONT).getSerializedName());
            capsInto(e, machine, P1, line);
            out.add(line);
        }
        out.write();
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Arquivo 3: config_lados_nbt
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static void nbtAll(Env e) {
        Out out = new Out("config_lados_nbt",
              "What the side configuration and the ejector write to the saved tile and to the item, and what reads them back.",
              "machine; variant (item_default: placed by a player with the plain item; set_block_default: placed by setBlock; hand_sides: patternA, the side i of each type gets the supported DataType of index (i + 1 + type ordinal) mod count; "
                    + "ejecting_items: item defaults plus ejecting on for ITEM; colors_strict: item defaults plus strict input, output color DARK_GREEN and input colors LEFT=RED and BOTTOM=DARK_BLUE; ejecting_all: ejecting on for every type, including those with canEject false); "
                    + "tile_nbt: component_config and component_ejector of tile.saveWithFullMetadata (present, json with the keys sorted and the numbers untyped, snbt with the tag types), saved_keys: all the top keys of the saved tag; "
                    + "item_components: tile.collectComponents() applied to new ItemStack(item): side_config and ejector encoded by their codecs, patch (the component patch keys, '!' prefix = removed), stack_equals_new (isSameItemSameComponents against the plain item) and stack_equals_new_ignoring_owner (the same without the mekanism:owner component a player-placed machine leaves); "
                    + "new_item: the plain item; loaded_tile: a second tile from BlockEntity.loadStatic of that tag, attached to the world at another position (setBlock then setBlockEntity, the way a chunk load does), with state (facing, control_type, config, ejecting, ejector) and caps; "
                    + "placed_from_item: a player placing an item that carries the side_config and ejector of the tile, with state and caps; broken: Level.destroyBlock(pos, true) on the original tile, the item entities dropped "
                    + "(count, side_config, ejector, patch), stacks_with_new, stacks_with_new_ignoring_owner, stacks_with_item_default_drop (isSameItemSameComponents against the drop of the item_default variant).");
        for (Spec spec : MACHINES) {
            ItemStack itemDefaultDrop = null;
            for (String variant : List.of("item_default", "set_block_default", "hand_sides", "ejecting_items", "colors_strict", "ejecting_all")) {
                TileEntityConfigurableMachine machine = machine(variant.equals("set_block_default") ? e.setBlock(spec, P1, Direction.NORTH) : e.useOn(spec, P1, Direction.NORTH));
                switch (variant) {
                    case "hand_sides" -> patternA(machine);
                    case "ejecting_items" -> machine.getConfig().getConfig(TransmissionType.ITEM).setEjecting(true);
                    case "colors_strict" -> ejectorA(machine);
                    case "ejecting_all" -> {
                        for (TransmissionType type : machine.getConfig().getTransmissions()) {
                            machine.getConfig().getConfig(type).setEjecting(true);
                        }
                    }
                    default -> {
                    }
                }
                JsonObject line = new JsonObject();
                line.addProperty("machine", spec.name);
                line.addProperty("variant", variant);
                line.add("state", stateJson(machine));
                // (i)
                CompoundTag tag = machine.saveWithFullMetadata(e.level().registryAccess());
                JsonObject tileNbt = new JsonObject();
                tileNbt.add("component_config", nbtSub(tag, "component_config"));
                tileNbt.add("component_ejector", nbtSub(tag, "component_ejector"));
                List<String> keys = new ArrayList<>(tag.keySet());
                Collections.sort(keys);
                tileNbt.add("saved_keys", strings(keys));
                line.add("tile_nbt", tileNbt);
                // (ii)
                DataComponentMap components = machine.collectComponents();
                ItemStack carried = new ItemStack(spec.item.get());
                carried.applyComponents(components);
                ItemStack fresh = new ItemStack(spec.item.get());
                JsonObject itemComponents = itemJson(e, carried);
                itemComponents.addProperty("collected_side_config", encode(e, AttachedSideConfig.CODEC, components.get(MekanismDataComponents.SIDE_CONFIG)).toString());
                itemComponents.addProperty("collected_ejector", encode(e, AttachedEjector.CODEC, components.get(MekanismDataComponents.EJECTOR)).toString());
                itemComponents.addProperty("stack_equals_new", ItemStack.isSameItemSameComponents(carried, fresh));
                itemComponents.addProperty("stack_equals_new_ignoring_owner", ItemStack.isSameItemSameComponents(withoutOwner(carried), fresh));
                line.add("item_components", itemComponents);
                line.add("new_item", itemJson(e, fresh));
                // (iii) o tile relido
                BlockState state = e.level().getBlockState(e.abs(P1));
                BlockEntity loaded = BlockEntity.loadStatic(e.abs(P2), state, tag, e.level().registryAccess());
                if (!(loaded instanceof TileEntityMekanism loadedTile) || loaded == machine) {
                    throw new IllegalStateException("loadStatic gave " + loaded);
                }
                e.clear(P2);
                e.helper.setBlock(P2, state);
                e.level().setBlockEntity(loadedTile);
                if (e.level().getBlockEntity(e.abs(P2)) != loadedTile) {
                    throw new IllegalStateException("the reloaded tile was not attached");
                }
                e.prepare(loadedTile);
                JsonObject loadedLine = new JsonObject();
                loadedLine.add("state", stateJson(loadedTile));
                capsInto(e, loadedTile, P2, loadedLine);
                line.add("loaded_tile", loadedLine);
                e.clear(P2);
                // (iii) o item que carrega os componentes
                ItemStack carrying = new ItemStack(spec.item.get());
                carrying.set(MekanismDataComponents.SIDE_CONFIG, components.get(MekanismDataComponents.SIDE_CONFIG));
                carrying.set(MekanismDataComponents.EJECTOR, components.get(MekanismDataComponents.EJECTOR));
                TileEntityMekanism placed = e.useOn(spec, P2, Direction.NORTH, carrying);
                JsonObject placedLine = new JsonObject();
                placedLine.add("state", stateJson(placed));
                capsInto(e, placed, P2, placedLine);
                line.add("placed_from_item", placedLine);
                e.clear(P2);
                // (iv) o item que cai
                BlockPos abs = e.abs(P1);
                e.level().destroyBlock(abs, true);
                JsonArray drops = new JsonArray();
                JsonObject broken = new JsonObject();
                boolean stacksWithNew = false;
                boolean stacksWithNewIgnoringOwner = false;
                boolean stacksWithDefaultDrop = false;
                for (ItemEntity entity : e.level().getEntitiesOfClass(ItemEntity.class, new AABB(abs).inflate(3))) {
                    ItemStack drop = entity.getItem();
                    drops.add(itemJson(e, drop));
                    if (variant.equals("item_default")) {
                        itemDefaultDrop = drop.copy();
                    }
                    if (ItemStack.isSameItemSameComponents(drop, fresh)) {
                        stacksWithNew = true;
                    }
                    if (ItemStack.isSameItemSameComponents(withoutOwner(drop), fresh)) {
                        stacksWithNewIgnoringOwner = true;
                    }
                    if (itemDefaultDrop != null && ItemStack.isSameItemSameComponents(drop, itemDefaultDrop)) {
                        stacksWithDefaultDrop = true;
                    }
                    entity.discard();
                }
                broken.add("drops", drops);
                broken.addProperty("stacks_with_new", stacksWithNew);
                broken.addProperty("stacks_with_new_ignoring_owner", stacksWithNewIgnoringOwner);
                broken.addProperty("stacks_with_item_default_drop", stacksWithDefaultDrop);
                line.add("broken", broken);
                out.add(line);
            }
        }
        out.write();
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Arquivo 4: config_lados_card
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static InteractionResult useCard(Env e, BlockPos rel, ItemStack card, boolean shift) {
        e.player.setShiftKeyDown(shift);
        e.player.setItemInHand(InteractionHand.MAIN_HAND, card);
        BlockPos abs = e.abs(rel);
        BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(abs).relative(Direction.UP, 0.5), Direction.UP, abs, false);
        InteractionResult result = card.useOn(new UseOnContext(e.level(), e.player, InteractionHand.MAIN_HAND, card, hit));
        e.player.setShiftKeyDown(false);
        return result;
    }

    private static void cardAll(Env e) {
        Out out = new Out("config_lados_card",
              "writeConfigurationData of a configured machine and setConfigurationData on a second machine of the same type, through the real configuration card (shift use to read, plain use to write), with the data cut in four ways.",
              "source: the machine A, placed by a player, with patternA sides (see config_lados_nbt), ejecting on for ITEM, ejector strict_input true, output color DARK_GREEN, input colors LEFT=RED and BOTTOM=DARK_BLUE, redstone mode HIGH. "
                    + "target B before: item defaults, ejecting off, ejector strict_input false, output color YELLOW, input color TOP=PINK, redstone mode LOW. "
                    + "write_direct: writeConfigurationData into a TagValueOutput (json, snbt); card_data: the CONFIGURATION_DATA the card stored (json, snbt), same_as_write_direct: its config child equals write_direct. "
                    + "case lines: case (full, no_ejector: the component_ejector child removed from the card data, no_config: component_config removed, no_control_type: the control_type key removed from the config child, no_config_child: the whole config child removed), "
                    + "result (the InteractionResult class and consumesAction), before and after (state: facing, control_type, config, ejecting, ejector) and caps_after (item/energy/chemical like config_lados_caps). "
                    + "direct_set_only line: setConfigurationData without configurationDataSet, caps before and after configurationDataSet. compat lines: isConfigurationDataCompatible(block of the other machine) for every pair.");
        for (Spec spec : MACHINES) {
            TileEntityConfigurableMachine source = machine(e.useOn(spec, P1, Direction.NORTH));
            patternA(source);
            source.getConfig().getConfig(TransmissionType.ITEM).setEjecting(true);
            ejectorA(source);
            source.setControlType(RedstoneControl.HIGH);
            JsonObject head = new JsonObject();
            head.addProperty("machine", spec.name);
            head.addProperty("step", "source");
            head.add("source", stateJson(source));
            TagValueOutput direct = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, e.level().registryAccess());
            source.writeConfigurationData(direct, e.player);
            CompoundTag written = direct.buildResult();
            head.add("write_direct", nbtSub(wrap(written), "x"));
            ItemStack card = new ItemStack((ItemLike) MekanismItems.CONFIGURATION_CARD);
            InteractionResult stored = useCard(e, P1, card, true);
            CompoundTag cardData = card.get(MekanismDataComponents.CONFIGURATION_DATA).configuration();
            head.addProperty("shift_result", stored.getClass().getSimpleName() + ":" + stored.consumesAction());
            JsonObject cardJson = new JsonObject();
            cardJson.addProperty("snbt", cardData == null ? "" : cardData.toString());
            cardJson.add("json", cardData == null ? com.google.gson.JsonNull.INSTANCE : nbtJson(cardData));
            head.add("card_data", cardJson);
            head.addProperty("same_as_write_direct", cardData != null && written.equals(cardData.get("config")));
            out.add(head);
            for (String caseName : List.of("full", "no_ejector", "no_config", "no_control_type", "no_config_child")) {
                CompoundTag data = cardData.copy();
                CompoundTag config = data.get("config") instanceof CompoundTag c ? c : null;
                switch (caseName) {
                    case "no_ejector" -> config.remove("component_ejector");
                    case "no_config" -> config.remove("component_config");
                    case "no_control_type" -> config.remove("control_type");
                    case "no_config_child" -> data.remove("config");
                    default -> {
                    }
                }
                TileEntityConfigurableMachine target = machine(e.useOn(spec, P2, Direction.NORTH));
                ejectorB(target);
                target.setControlType(RedstoneControl.LOW);
                JsonObject line = new JsonObject();
                line.addProperty("machine", spec.name);
                line.addProperty("case", caseName);
                line.add("before", stateJson(target));
                ItemStack usedCard = new ItemStack((ItemLike) MekanismItems.CONFIGURATION_CARD);
                usedCard.set(MekanismDataComponents.CONFIGURATION_DATA, new ConfigurationData(data));
                InteractionResult result = useCard(e, P2, usedCard, false);
                line.addProperty("result", result.getClass().getSimpleName() + ":" + result.consumesAction());
                line.add("after", stateJson(target));
                JsonObject caps = new JsonObject();
                capsInto(e, target, P2, caps);
                line.add("caps_after", caps);
                out.add(line);
                e.clear(P2);
            }
            // setConfigurationData sem o configurationDataSet que o cartão chama depois
            TileEntityConfigurableMachine target = machine(e.useOn(spec, P2, Direction.NORTH));
            ejectorB(target);
            target.setControlType(RedstoneControl.LOW);
            JsonObject direct2 = new JsonObject();
            direct2.addProperty("machine", spec.name);
            direct2.addProperty("step", "direct_set_only");
            JsonObject capsBefore = new JsonObject();
            capsInto(e, target, P2, capsBefore);
            direct2.add("caps_before", capsBefore);
            ValueInput input = TagValueInput.create(ProblemReporter.DISCARDING, e.level().registryAccess(), (CompoundTag) cardData.get("config"));
            target.setConfigurationData(input, e.player);
            direct2.add("state_after_set", stateJson(target));
            JsonObject capsSet = new JsonObject();
            capsInto(e, target, P2, capsSet);
            direct2.add("caps_after_set", capsSet);
            target.configurationDataSet();
            JsonObject capsDone = new JsonObject();
            capsInto(e, target, P2, capsDone);
            direct2.add("caps_after_configuration_data_set", capsDone);
            out.add(direct2);
            e.clear(P2);
            e.clear(P1);
        }
        // compatibilidade
        for (Spec from : ALL) {
            TileEntityMekanism tile = e.useOn(from, P1, Direction.NORTH);
            JsonObject line = new JsonObject();
            line.addProperty("machine", from.name);
            line.addProperty("step", "compat");
            JsonObject to = new JsonObject();
            for (Spec other : ALL) {
                Block block = other.state.get().getBlock();
                to.addProperty(other.name, tile.isConfigurationDataCompatible(block));
            }
            line.add("compatible_with", to);
            out.add(line);
        }
        out.write();
    }

    private static CompoundTag wrap(CompoundTag tag) {
        CompoundTag wrapper = new CompoundTag();
        wrapper.put("x", tag);
        return wrapper;
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Arquivos 5: config_lados_eject_<cenário>
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final class Eject {

        final String scenario;
        final Spec spec;
        final String description;
        final Map<Integer, List<Runnable>> actions = new HashMap<>();
        final Map<Integer, List<String>> marksAt = new HashMap<>();
        final List<String> lines = new ArrayList<>();
        boolean viaNbt;
        private MekGameTestHelper helper;
        private Env env;
        private boolean armed;
        private boolean running;
        private boolean done;
        private int tick;
        private final List<String> marks = new ArrayList<>();

        Eject(String scenario, Spec spec, String description) {
            this.scenario = scenario;
            this.spec = spec;
            this.description = description;
        }

        void at(int tick, String mark, Runnable action) {
            actions.computeIfAbsent(tick, t -> new ArrayList<>()).add(() -> {
                marks.add(mark);
                action.run();
            });
        }

        void itemEjecting(int tick, boolean on) {
            at(tick, "item ejecting " + on, () -> machine().getConfig().getConfig(TransmissionType.ITEM).setEjecting(on));
        }

        void setItem(int tick, RelativeSide side, DataType dataType) {
            at(tick, "item " + side.getSerializedName() + "=" + name(dataType), () -> setMode(machine(), TransmissionType.ITEM, side, dataType));
        }

        void refillOutput(int... ticks) {
            for (int tick : ticks) {
                at(tick, "output clay_ball 64", () -> setSlot(spec.output, Items.CLAY_BALL, 64));
            }
        }

        void refillInput(int... ticks) {
            for (int tick : ticks) {
                at(tick, "input clay 5", () -> setSlot(spec.input, Items.CLAY, 5));
            }
        }

        void setSlot(int index, net.minecraft.world.item.Item item, int count) {
            machine().getInventorySlots().get(index).setContents(ItemResource.of(item), count, null);
        }

        TileEntityConfigurableMachine machine() {
            return (TileEntityConfigurableMachine) env.tile(P1);
        }

        void begin(MekGameTestHelper helper) {
            this.helper = helper;
            this.env = new Env(helper);
            TileEntityConfigurableMachine placed = ConfigLadosTraceTest.machine(env.useOn(spec, P1, Direction.NORTH));
            if (viaNbt) {
                CompoundTag tag = placed.saveWithFullMetadata(helper.getLevel().registryAccess());
                CompoundTag config = (CompoundTag) tag.get("component_config");
                config.putBoolean("eject0", true);
                config.putBoolean("eject3", false);
                BlockState state = helper.getLevel().getBlockState(helper.absolutePos(P1));
                BlockEntity loaded = BlockEntity.loadStatic(helper.absolutePos(P1), state, tag, helper.getLevel().registryAccess());
                env.clear(P1);
                helper.setBlock(P1, state);
                helper.getLevel().setBlockEntity((TileEntityMekanism) loaded);
                if (env.tile(P1) != loaded) {
                    throw new IllegalStateException("the reloaded tile was not attached");
                }
            }
            for (Direction face : Direction.values()) {
                helper.setBlock(P1.relative(face), Blocks.CHEST.defaultBlockState());
            }
            JsonObject meta = new JsonObject();
            meta.addProperty("commit", COMMIT);
            meta.addProperty("date", DATE);
            meta.addProperty("file", "config_lados_eject_" + scenario + ".jsonl");
            meta.addProperty("machine", spec.name);
            meta.addProperty("scenario", description);
            meta.addProperty("fields", "t: ticks after the first tick the scenario ran (the actions of tick t run in ServerTickEvent.Pre, the line is written in ServerTickEvent.Post, after the machine ticked); marks: the actions of the tick; "
                  + "facing; chests {world face: {item: count}}: what each of the 6 chests touching the machine holds (omitted faces are empty); output, input (and infusion, tank for the infuser): [item, count] of the slots, '' when empty; "
                  + "tick_delay: TileComponentEjector.tickDelay read by reflection; ejecting {type: bool}; config: the ITEM sides (front,left,right,back,top,bottom).");
            JsonObject metaLine = new JsonObject();
            metaLine.add("meta", meta);
            lines.add(metaLine.toString());
            armed = true;
        }

        void pre() {
            if (!armed || done) {
                return;
            }
            running = true;
            List<Runnable> list = actions.get(tick);
            if (list != null) {
                list.forEach(Runnable::run);
            }
        }

        void post() {
            if (!running || done) {
                return;
            }
            record();
            marks.clear();
            tick++;
            if (tick >= EJECT_TICKS) {
                done = true;
            }
        }

        private JsonArray slot(int index) {
            JsonArray array = new JsonArray();
            ItemStack stack = machine().getInventorySlots().get(index).asItemStack();
            array.add(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath());
            array.add(stack.getCount());
            return array;
        }

        private void record() {
            TileEntityConfigurableMachine machine = machine();
            JsonObject line = new JsonObject();
            line.addProperty("t", tick);
            line.add("marks", strings(marks));
            line.addProperty("facing", machine.getDirection().getSerializedName());
            JsonObject chests = new JsonObject();
            for (Direction face : Direction.values()) {
                BlockEntity entity = helper.getLevel().getBlockEntity(helper.absolutePos(P1.relative(face)));
                if (entity instanceof Container container) {
                    Map<String, Integer> totals = new java.util.TreeMap<>();
                    for (int i = 0; i < container.getContainerSize(); i++) {
                        ItemStack stack = container.getItem(i);
                        if (!stack.isEmpty()) {
                            totals.merge(BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath(), stack.getCount(), Integer::sum);
                        }
                    }
                    if (!totals.isEmpty()) {
                        JsonObject object = new JsonObject();
                        totals.forEach(object::addProperty);
                        chests.add(face.getSerializedName(), object);
                    }
                }
            }
            line.add("chests", chests);
            line.add("output", slot(spec.output));
            line.add("input", slot(spec.input));
            if (spec.infusion >= 0) {
                line.add("infusion", slot(spec.infusion));
                mekanism.common.tile.machine.TileEntityMetallurgicInfuser infuser = (mekanism.common.tile.machine.TileEntityMetallurgicInfuser) machine;
                JsonArray tank = new JsonArray();
                tank.add(infuser.infusionTank.resource().isEmpty() ? "" : infuser.infusionTank.resource().typeHolder().getRegisteredName());
                tank.add(infuser.infusionTank.amountAsLong());
                line.add("tank", tank);
            }
            line.addProperty("tick_delay", (int) field(machine.getEjector(), TileComponentEjector.class, "tickDelay"));
            line.add("ejecting", ejectingJson(machine));
            JsonArray sides = new JsonArray();
            for (RelativeSide side : RelativeSide.VALUES) {
                sides.add(name(machine.getConfig().getConfig(TransmissionType.ITEM).getDataType(side)));
            }
            line.add("config", sides);
            lines.add(line.toString());
        }

        void finish() {
            try {
                Path file = Path.of("trace", "config_lados_eject_" + scenario + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }
}
