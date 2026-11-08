package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;
import mekanism.common.block.attribute.Attribute;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.interfaces.IRedstoneControl.RedstoneControl;
import mekanism.common.tile.prefab.TileEntityProgressMachine;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.LeverBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.ChestBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.AttachFace;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava, a cada tick e no meio dele, o campo `redstone` de cada máquina (e o que ele manda: `canFunction`, a atividade, o progresso) quando o sinal dos
/// vizinhos muda de jeitos que avisam a máquina (`neighborChanged` pelo `updateNeighborsAt`, o `onNeighborChange` do NeoForge pelo
/// `updateNeighbourForOutputSignal`, `setPlacedBy` -> `onAdded`) e de jeitos que não avisam (`setBlock` sem a flag 1, bloco posto pelo gabarito ou por
/// `setBlock`, chunk descarregado).
/// Cada linha de `neighbors_<cenário>.jsonl` é um tick (`ServerTickEvent.Post`), com `rows` (uma entrada por máquina) e `ops`: as operações do tick,
/// cada uma com o `redstone` de todas as máquinas logo depois dela (a leitura no meio do tick). As operações vêm em dois momentos: `pre`
/// (`ServerTickEvent.Pre`, antes do tick da máquina) e `step` (o passo do gameTest, entre os block entities e o `ServerTickEvent.Post`).
/// Linha 0 de cada arquivo: as ordens do jogo (`NeighborUpdater.UPDATE_ORDER`, `Level.NEIGHBOR_UPDATE_LIST`) lidas por reflexão.
///
/// Geometria (cada linha `r` do cenário, `z = 4r + 2`): a máquina em (3, 1, z); a fonte de sinal a oeste, (2, 1, z); o "cutucão" (um bloco sem
/// sinal, que só serve para avisar a máquina) a leste, (4, 1, z); o vizinho com block entity (baú ou outra máquina) ao norte, (3, 1, z - 1).
@ForEachTest(groups = "trace")
public class NeighborTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final int ROW_STRIDE = 4;

    private record Kind(String name, Supplier<BlockState> state, Supplier<ItemLike> item) {
    }

    private static final Kind CHAMBER = new Kind("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.defaultState(),
          () -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem());
    private static final Kind CRUSHER = new Kind("crusher", () -> MekanismBlocks.CRUSHER.defaultState(), () -> MekanismBlocks.CRUSHER.asItem());
    private static final Kind SMELTER = new Kind("energized_smelter", () -> MekanismBlocks.ENERGIZED_SMELTER.defaultState(),
          () -> MekanismBlocks.ENERGIZED_SMELTER.asItem());
    private static final Kind INFUSER = new Kind("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.defaultState(),
          () -> MekanismBlocks.METALLURGIC_INFUSER.asItem());
    private static final Kind HEAT = new Kind("heat_generator", () -> GeneratorsBlocks.HEAT_GENERATOR.defaultState(),
          () -> GeneratorsBlocks.HEAT_GENERATOR.asItem());

    private static BlockPos subject(int row) {
        return new BlockPos(3, 1, ROW_STRIDE * row + 2);
    }

    private static BlockPos source(int row) {
        return new BlockPos(2, 1, ROW_STRIDE * row + 2);
    }

    private static BlockPos poke(int row) {
        return new BlockPos(4, 1, ROW_STRIDE * row + 2);
    }

    private static BlockPos other(int row) {
        return new BlockPos(3, 1, ROW_STRIDE * row + 1);
    }

    /// `rows` máquinas do mesmo tipo, cada uma com a fonte já posta onde `powered` diz (no gabarito, sem avisar ninguém), o baú ao norte quando `chest`.
    private static void template(DynamicTest test, Kind kind, int rows, boolean[] powered, boolean chest) {
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(7, 3, ROW_STRIDE * rows + 1)
                  .fill(0, 0, 0, 6, 0, ROW_STRIDE * rows, Blocks.STONE.defaultBlockState());
            for (int row = 0; row < rows; row++) {
                BlockPos at = subject(row);
                builder = builder.fill(at.getX(), at.getY(), at.getZ(), at.getX(), at.getY(), at.getZ(), kind.state.get());
                if (powered != null && powered[row]) {
                    BlockPos from = source(row);
                    builder = builder.fill(from.getX(), from.getY(), from.getZ(), from.getX(), from.getY(), from.getZ(), Blocks.REDSTONE_BLOCK.defaultBlockState());
                }
                if (chest) {
                    BlockPos next = other(row);
                    builder = builder.fill(next.getX(), next.getY(), next.getZ(), next.getX(), next.getY(), next.getZ(), Blocks.CHEST.defaultBlockState());
                }
            }
            return builder;
        });
    }

    /// Um cenário: uma lista de passos, cada um `(ticks depois do anterior, "pre" ou "step", operações separadas por ';')`.
    private static void script(DynamicTest test, String name, Kind kind, int rows, boolean[] powered, boolean chest, int end, String... steps) {
        template(test, kind, rows, powered, chest);
        Scene scene = new Scene(name, kind, rows);
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> scene.pre());
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> scene.begin(helper));
            int tick = 0;
            for (String step : steps) {
                String[] parts = step.split("\\|", 3);
                int at = Integer.parseInt(parts[0].trim());
                String phase = parts[1].trim();
                String ops = parts[2].trim();
                if (phase.equals("pre")) {
                    scene.schedulePre(at, ops);
                } else {
                    sequence.thenExecuteAfter(at - tick, () -> scene.apply(ops, "step"));
                    tick = at;
                }
            }
            sequence.thenExecuteAfter(end - tick, scene::write);
            sequence.thenSucceed();
        });
    }

    private static final String SETUP = "5|step|mode 0 HIGH;mode 1 LOW;mode 2 DISABLED;fill 0;fill 1;fill 2";

    // ---- o aviso comum: bloco de redstone e alavanca ----

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Enrichment Chamber in the three redstone modes while a redstone block and a lever change the signal and notify the neighbours, in the step phase and before the tick of the machine.")
    public static void chamberNotified(final DynamicTest test) {
        script(test, "chamber_notified", CHAMBER, 3, null, false, 120,
              SETUP,
              "10|step|src 0 on 3;src 1 on 3;src 2 on 3",
              "25|step|src 0 off 3;src 1 off 3;src 2 off 3",
              "35|pre|src 0 on 3;src 1 on 3;src 2 on 3",
              "45|pre|src 0 off 3;src 1 off 3;src 2 off 3",
              "55|step|lever 0 place 3;lever 1 place 3;lever 2 place 3",
              "60|step|lever 0 pull 3;lever 1 pull 3;lever 2 pull 3",
              "75|step|lever 0 pull 3;lever 1 pull 3;lever 2 pull 3");
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Heat Generator in the three redstone modes while a redstone block and a lever change the signal and notify the neighbours.")
    public static void heatNotified(final DynamicTest test) {
        script(test, "heat_notified", HEAT, 3, null, false, 120,
              SETUP,
              "10|step|src 0 on 3;src 1 on 3;src 2 on 3",
              "25|step|src 0 off 3;src 1 off 3;src 2 off 3",
              "35|pre|src 0 on 3;src 1 on 3;src 2 on 3",
              "45|pre|src 0 off 3;src 1 off 3;src 2 off 3",
              "55|step|lever 0 place 3;lever 1 place 3;lever 2 place 3",
              "60|step|lever 0 pull 3;lever 1 pull 3;lever 2 pull 3",
              "75|step|lever 0 pull 3;lever 1 pull 3;lever 2 pull 3");
    }

    // ---- a mudança que não avisa ----

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Enrichment Chamber while the signal changes with setBlock flags that do not notify the neighbours, then another change that does notify it, and a notified change undone in the same tick by one that does not.")
    public static void chamberUnnotified(final DynamicTest test) {
        script(test, "chamber_unnotified", CHAMBER, 3, null, false, 150,
              SETUP,
              "10|step|src 0 on 2;src 1 on 2;src 2 on 2",
              "30|step|poke 0 3;poke 1 3;poke 2 3",
              "45|step|src 0 off 2;src 1 off 2;src 2 off 2",
              "60|step|poke 0 3;poke 1 3;poke 2 3",
              "70|step|src 0 on 818;src 1 on 818;src 2 on 818",
              "80|step|poke 0 1;poke 1 1;poke 2 1",
              "90|step|src 0 off 1;src 1 off 1;src 2 off 1",
              "100|step|src 0 on 3;src 1 on 3;src 2 on 3;src 0 off 2;src 1 off 2;src 2 off 2",
              "115|step|poke 0 3;poke 1 3;poke 2 3");
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Heat Generator while the signal changes with setBlock flags that do not notify the neighbours, then another change that does notify it.")
    public static void heatUnnotified(final DynamicTest test) {
        script(test, "heat_unnotified", HEAT, 3, null, false, 150,
              SETUP,
              "10|step|src 0 on 2;src 1 on 2;src 2 on 2",
              "30|step|poke 0 3;poke 1 3;poke 2 3",
              "45|step|src 0 off 2;src 1 off 2;src 2 off 2",
              "60|step|poke 0 3;poke 1 3;poke 2 3",
              "70|step|src 0 on 818;src 1 on 818;src 2 on 818",
              "80|step|poke 0 1;poke 1 1;poke 2 1",
              "90|step|src 0 off 1;src 1 off 1;src 2 off 1",
              "100|step|src 0 on 3;src 1 on 3;src 2 on 3;src 0 off 2;src 1 off 2;src 2 off 2",
              "115|step|poke 0 3;poke 1 3;poke 2 3");
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the signal changing in the middle of the tick: notified in the tick of the machine before it (pre) and after it (step), and a change that hides the signal inside the same tick that showed it.")
    public static void chamberMidTick(final DynamicTest test) {
        script(test, "chamber_mid_tick", CHAMBER, 1, null, false, 80,
              "5|step|mode 0 HIGH;fill 0",
              "10|pre|src 0 on 3",
              "20|step|src 0 off 3",
              "30|pre|src 0 on 3;src 0 off 2",
              "31|step|poke 0 3",
              "40|step|src 0 on 3",
              "40|pre|poke 0 3",
              "50|pre|src 0 off 3;src 0 on 2",
              "51|step|poke 0 3");
    }

    // ---- a máquina posta já com sinal ----

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Enrichment Chamber put next to a powered redstone block by the template, by setBlock and by a player (setPlacedBy, onAdded), and what a later notification does.")
    public static void chamberPlaced(final DynamicTest test) {
        script(test, "chamber_placed", CHAMBER, 3, new boolean[]{true, true, true}, false, 110,
              "5|step|mode 0 HIGH;fill 0",
              "10|step|poke 0 3",
              "20|step|clear 1;setblock 1 3;mode 1 HIGH;fill 1",
              "30|step|poke 1 3",
              "40|step|clear 2;place 2;mode 2 HIGH;fill 2",
              "60|step|src 2 off 3",
              "75|step|clear 2;src 2 on 3;place 2");
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Heat Generator put next to a powered redstone block by the template, by setBlock and by a player (setPlacedBy, onAdded), and what a later notification does.")
    public static void heatPlaced(final DynamicTest test) {
        script(test, "heat_placed", HEAT, 3, new boolean[]{true, true, true}, false, 110,
              "5|step|mode 0 HIGH;fill 0",
              "10|step|poke 0 3",
              "20|step|clear 1;setblock 1 3;mode 1 HIGH;fill 1",
              "30|step|poke 1 3",
              "40|step|clear 2;place 2;mode 2 HIGH;fill 2",
              "60|step|src 2 off 3",
              "75|step|clear 2;src 2 on 3;place 2");
    }

    // ---- chunk descarregado e salvar ----

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Enrichment Chamber saved and loaded again (redstone is in the NBT) and with the signal changed while it was out of the chunk: the loaded value is the saved one until a neighbour notifies it.")
    public static void chamberReload(final DynamicTest test) {
        script(test, "chamber_reload", CHAMBER, 3, null, false, 150,
              "5|step|mode 0 HIGH;mode 1 HIGH;mode 2 LOW;fill 0;fill 1;fill 2",
              "10|step|src 0 on 3;src 1 on 3;src 2 on 3",
              "20|step|reload 0;reload 1;reload 2",
              "30|step|unload 0;unload 1;unload 2",
              "32|step|src 0 off 3;src 1 off 3;src 2 off 3",
              "35|step|load 0;load 1;load 2",
              "50|step|poke 0 3;poke 1 3;poke 2 3",
              "60|step|unload 0;unload 1;unload 2",
              "62|step|src 0 on 3;src 1 on 3;src 2 on 3",
              "65|step|load 0;load 1;load 2",
              "80|step|poke 0 3;poke 1 3;poke 2 3");
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Heat Generator saved and loaded again (redstone is in the NBT) and with the signal changed while it was out of the chunk.")
    public static void heatReload(final DynamicTest test) {
        script(test, "heat_reload", HEAT, 3, null, false, 150,
              "5|step|mode 0 HIGH;mode 1 HIGH;mode 2 LOW;fill 0;fill 1;fill 2",
              "10|step|src 0 on 3;src 1 on 3;src 2 on 3",
              "20|step|reload 0;reload 1;reload 2",
              "30|step|unload 0;unload 1;unload 2",
              "32|step|src 0 off 3;src 1 off 3;src 2 off 3",
              "35|step|load 0;load 1;load 2",
              "50|step|poke 0 3;poke 1 3;poke 2 3",
              "60|step|unload 0;unload 1;unload 2",
              "62|step|src 0 on 3;src 1 on 3;src 2 on 3",
              "65|step|load 0;load 1;load 2",
              "80|step|poke 0 3;poke 1 3;poke 2 3");
    }

    // ---- onNeighborChange (NeoForge): block entity vizinho mudou ----

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Enrichment Chamber with a stale redstone value fixed by a chest next to it changing (BlockEntity.setChanged -> updateNeighbourForOutputSignal -> onNeighborChange), and by another machine next to it changing its comparator.")
    public static void chamberOutputSignal(final DynamicTest test) {
        script(test, "chamber_output_signal", CHAMBER, 2, null, true, 120,
              "5|step|mode 0 HIGH;mode 1 HIGH;fill 0;fill 1;other 1 chamber",
              "10|step|src 0 on 2;src 1 on 2",
              "20|step|chest 0 changed",
              "30|step|src 0 off 2;src 1 off 2",
              "40|step|chest 0 changed",
              "50|step|src 0 on 2;src 1 on 2",
              "55|step|ninput 1",
              "70|step|src 0 off 2;src 1 off 2",
              "75|step|nclear 1");
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Heat Generator with a stale redstone value fixed by a chest next to it changing.")
    public static void heatOutputSignal(final DynamicTest test) {
        script(test, "heat_output_signal", HEAT, 2, null, true, 120,
              "5|step|mode 0 HIGH;mode 1 HIGH;fill 0;fill 1",
              "10|step|src 0 on 2;src 1 on 2",
              "20|step|chest 0 changed",
              "30|step|src 0 off 2;src 1 off 2",
              "40|step|chest 0 changed");
    }

    // ---- as outras três máquinas usam o mesmo caminho ----

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Crusher with the notified, unnotified and placed cases of the Enrichment Chamber.")
    public static void crusherCore(final DynamicTest test) {
        script(test, "crusher_core", CRUSHER, 3, new boolean[]{false, false, true}, false, 120, core());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Energized Smelter with the notified, unnotified and placed cases of the Enrichment Chamber.")
    public static void smelterCore(final DynamicTest test) {
        script(test, "smelter_core", SMELTER, 3, new boolean[]{false, false, true}, false, 120, core());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the Metallurgic Infuser with the notified, unnotified and placed cases of the Enrichment Chamber.")
    public static void infuserCore(final DynamicTest test) {
        script(test, "infuser_core", INFUSER, 3, new boolean[]{false, false, true}, false, 120, core());
    }

    private static String[] core() {
        return new String[]{
              "5|step|mode 0 HIGH;mode 1 HIGH;mode 2 HIGH;fill 0;fill 1;fill 2",
              "10|step|src 0 on 3;src 1 on 2",
              "25|step|poke 1 3;poke 2 3",
              "40|step|src 0 off 3;src 1 off 2;src 2 off 2",
              "55|step|poke 1 3;poke 2 3",
              "70|step|clear 2;src 2 on 3;place 2;mode 2 HIGH;fill 2"};
    }

    // ---- a prova de que as cinco usam BlockTile.neighborChanged ----

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_neighbors")
    @TestHolder(description = "Records the block class and the class that declares neighborChanged and onNeighborChange for the five blocks.")
    public static void blockClasses(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(1, 1, 1));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            List<String> lines = new ArrayList<>();
            for (Kind kind : List.of(CHAMBER, CRUSHER, SMELTER, INFUSER, HEAT)) {
                Block block = kind.state.get().getBlock();
                lines.add("{\"machine\":\"" + kind.name + "\",\"block_class\":\"" + block.getClass().getName() + "\",\"neighbor_changed\":\""
                      + declaring(block.getClass(), "neighborChanged") + "\",\"on_neighbor_change\":\"" + declaring(block.getClass(), "onNeighborChange") + "\"}");
            }
            write("neighbors_block_classes", lines);
        }).thenSucceed());
    }

    private static String declaring(Class<?> type, String method) {
        for (Class<?> current = type; current != null; current = current.getSuperclass()) {
            for (java.lang.reflect.Method declared : current.getDeclaredMethods()) {
                if (declared.getName().equals(method)) {
                    return current.getName();
                }
            }
        }
        return "";
    }

    private static void write(String name, List<String> lines) {
        try {
            Path file = Path.of("trace", name + ".jsonl");
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static String directions(Direction[] order) {
        StringBuilder out = new StringBuilder("[");
        for (int i = 0; i < order.length; i++) {
            out.append(i == 0 ? "" : ",").append('"').append(order[i].getName()).append('"');
        }
        return out.append(']').toString();
    }

    private static final class Scene {
        private final String name;
        private final Kind kind;
        private final int rows;
        private final List<String> lines = new ArrayList<>();
        private final List<String> ops = new ArrayList<>();
        private final List<String> marks = new ArrayList<>();
        private final Map<Integer, List<String>> preOps = new HashMap<>();
        private final Map<Integer, CompoundTag> unloaded = new HashMap<>();
        private final Map<Integer, Boolean> pokes = new HashMap<>();
        private MekGameTestHelper helper;
        private Player player;
        private boolean recording;
        private int tick;

        Scene(String name, Kind kind, int rows) {
            this.name = name;
            this.kind = kind;
            this.rows = rows;
        }

        void begin(MekGameTestHelper helper) {
            this.helper = helper;
            this.player = helper.makeMockPlayerLookingAt(new BlockPos(3, 0, 2), Direction.UP);
            Direction[] updateOrder = net.minecraft.world.level.redstone.NeighborUpdater.UPDATE_ORDER;
            Direction[] outputOrder;
            try {
                java.lang.reflect.Field field = net.minecraft.world.level.Level.class.getDeclaredField("NEIGHBOR_UPDATE_LIST");
                field.setAccessible(true);
                outputOrder = (Direction[]) field.get(null);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
            lines.add("{\"machine\":\"" + kind.name + "\",\"rows\":" + rows + ",\"update_order\":" + directions(updateOrder) + ",\"output_signal_order\":"
                  + directions(outputOrder) + "}");
            recording = true;
        }

        void schedulePre(int at, String text) {
            preOps.computeIfAbsent(at, key -> new ArrayList<>()).add(text);
        }

        void pre() {
            if (!recording) {
                return;
            }
            List<String> list = preOps.get(tick);
            if (list != null) {
                for (String text : list) {
                    apply(text, "pre");
                }
            }
        }

        private BlockPos abs(BlockPos pos) {
            return helper.absolutePos(pos);
        }

        private TileEntityMekanism tile(int row) {
            return helper.getLevel().getBlockEntity(abs(subject(row))) instanceof TileEntityMekanism tile ? tile : null;
        }

        void apply(String text, String phase) {
            for (String op : text.split(";")) {
                run(op.trim());
                StringBuilder entry = new StringBuilder("{\"op\":\"").append(op.trim()).append("\",\"phase\":\"").append(phase).append("\",\"powered\":[");
                for (int row = 0; row < rows; row++) {
                    TileEntityMekanism tile = tile(row);
                    entry.append(row == 0 ? "" : ",").append(tile == null ? "null" : String.valueOf(tile.isPowered()));
                }
                ops.add(entry.append("]}").toString());
            }
        }

        private void run(String op) {
            String[] words = op.split(" ");
            int row = words.length > 1 ? Integer.parseInt(words[1]) : 0;
            var level = helper.getLevel();
            switch (words[0]) {
                case "mode" -> tile(row).setControlType(RedstoneControl.valueOf(words[2]));
                case "fill" -> fill(tile(row));
                case "src" -> level.setBlock(abs(source(row)), words[2].equals("on") ? Blocks.REDSTONE_BLOCK.defaultBlockState() : Blocks.AIR.defaultBlockState(),
                      Integer.parseInt(words[3]));
                case "lever" -> lever(row, words[2], Integer.parseInt(words[3]));
                case "poke" -> {
                    boolean placed = pokes.merge(row, true, (old, ignored) -> !old);
                    level.setBlock(abs(poke(row)), placed ? Blocks.STONE.defaultBlockState() : Blocks.AIR.defaultBlockState(), Integer.parseInt(words[2]));
                }
                case "chest" -> ((ChestBlockEntity) level.getBlockEntity(abs(other(row)))).setChanged();
                case "other" -> level.setBlock(abs(other(row)), MekanismBlocks.ENRICHMENT_CHAMBER.defaultState(), 3);
                case "ninput" -> ((TileEntityMekanism) level.getBlockEntity(abs(other(row)))).getInventorySlots().get(0)
                      .setContents(ItemResource.of(Items.CLAY), 5, null);
                case "nclear" -> ((TileEntityMekanism) level.getBlockEntity(abs(other(row)))).getInventorySlots().get(0)
                      .setContents(ItemResource.EMPTY, 0, null);
                case "clear" -> level.setBlock(abs(subject(row)), Blocks.AIR.defaultBlockState(), 3);
                case "setblock" -> level.setBlock(abs(subject(row)), kind.state.get(), Integer.parseInt(words[2]));
                case "place" -> helper.useOn(subject(row).below(), new ItemStack(kind.item.get()), player, Direction.UP);
                case "unload" -> unload(row);
                case "load" -> load(row);
                case "reload" -> {
                    unload(row);
                    load(row);
                }
                default -> throw new IllegalStateException(op);
            }
        }

        private void lever(int row, String action, int flags) {
            var level = helper.getLevel();
            BlockPos at = abs(source(row));
            switch (action) {
                case "place" -> level.setBlock(at, Blocks.LEVER.defaultBlockState().setValue(LeverBlock.FACE, AttachFace.FLOOR), flags);
                case "pull" -> helper.pullLever(source(row));
                case "remove" -> level.setBlock(at, Blocks.AIR.defaultBlockState(), flags);
                default -> throw new IllegalStateException(action);
            }
        }

        private void fill(TileEntityMekanism tile) {
            if (tile instanceof TileEntityHeatGenerator heat) {
                heat.lavaTank.setContents(FluidResource.of(Fluids.LAVA), 10_000, null);
            } else if (tile instanceof TileEntityProgressMachine<?> machine) {
                Item input = switch (kind.name) {
                    case "crusher" -> Items.COBBLESTONE;
                    case "energized_smelter" -> Items.SAND;
                    case "metallurgic_infuser" -> Items.DIRT;
                    default -> Items.CLAY;
                };
                int inputSlot = kind.name.equals("metallurgic_infuser") ? 1 : 0;
                machine.getInventorySlots().get(inputSlot).setContents(ItemResource.of(input), 64, null);
                if (tile.getEnergyContainer() instanceof MachineEnergyContainer<?> energy) {
                    energy.setEnergy(20_000, null);
                }
            }
        }

        private void unload(int row) {
            TileEntityMekanism old = tile(row);
            BlockPos pos = abs(subject(row));
            unloaded.put(row, old.saveWithFullMetadata(helper.getLevel().registryAccess()));
            LevelChunk chunk = helper.getLevel().getChunkAt(pos);
            old.onChunkUnloaded();
            chunk.removeBlockEntity(pos);
        }

        private void load(int row) {
            BlockPos pos = abs(subject(row));
            CompoundTag tag = unloaded.remove(row);
            BlockEntity loaded = BlockEntity.loadStatic(pos, helper.getLevel().getBlockState(pos), tag, helper.getLevel().registryAccess());
            if (!(loaded instanceof TileEntityMekanism)) {
                throw new IllegalStateException("loadStatic gave " + loaded);
            }
            helper.getLevel().getChunkAt(pos).addAndRegisterBlockEntity(loaded);
        }

        void record() {
            if (!recording) {
                return;
            }
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++).append(",\"marks\":[],\"ops\":[");
            for (int i = 0; i < ops.size(); i++) {
                line.append(i == 0 ? "" : ",").append(ops.get(i));
            }
            ops.clear();
            line.append("],\"rows\":[");
            for (int row = 0; row < rows; row++) {
                line.append(row == 0 ? "" : ",");
                TileEntityMekanism tile = tile(row);
                if (tile == null) {
                    line.append("null");
                    continue;
                }
                line.append("{\"powered\":").append(tile.isPowered())
                      .append(",\"was_powered\":").append(tile.wasPowered())
                      .append(",\"can_function\":").append(tile.canFunction())
                      .append(",\"mode\":\"").append(tile.getControlType().name().toLowerCase()).append('"')
                      .append(",\"active\":").append(tile.getActive())
                      .append(",\"block_active\":").append(Attribute.isActive(tile.getBlockState()))
                      .append(",\"comparator\":").append(tile.getCurrentRedstoneLevel());
                if (tile instanceof TileEntityProgressMachine<?> machine) {
                    line.append(",\"progress\":").append(machine.getRecipeProgress());
                }
                if (tile instanceof TileEntityHeatGenerator heat) {
                    line.append(",\"energy\":").append(heat.energyContainer().getAmountAsLong()).append(",\"lava\":").append(heat.lavaTank.amountAsLong());
                } else if (tile.getEnergyContainer() instanceof MachineEnergyContainer<?> energy) {
                    line.append(",\"energy\":").append(energy.getAmountAsLong());
                }
                line.append('}');
            }
            lines.add(line.append("]}").toString());
        }

        void write() {
            recording = false;
            NeighborTraceTest.write("neighbors_" + name, lines);
        }
    }
}
