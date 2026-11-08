package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;
import mekanism.api.inventory.IInventorySlot;
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
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.ComparatorBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.ComparatorBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o comparador das máquinas. Dois cenários:
///
/// 1. `comparator_order_<ordem>`: duas Enrichment Chambers encostadas, N (a vizinha) e S (a que tem o `redstone` velho), cada uma com um comparador a leste. A
///    mudança de nível de N (`TileEntityMekanism.tickServer`, `:645-652`) avisa os vizinhos dentro do tick de N (`notifyComparatorChange` ->
///    `Level.updateNeighbourForOutputSignal` -> `onNeighborChange` -> `updatePower`), então S, se tica depois de N no mesmo tick, já vê o sinal novo;
///    se tica antes, vê no tick seguinte. A linha 0 põe N antes de S (N tica primeiro), a linha 1 põe S antes (S tica primeiro). Cada linha do rastro
///    leva o estado de S (`powered`, `can_function`, atividade, progresso, energia), o `currentRedstoneLevel` de N e a saída dos dois comparadores.
/// 2. `comparator_levels_<máquina>`: uma máquina e um comparador encostado, com o conteúdo mudado em passos (vazia, itens no slot de entrada, saída cheia,
///    todos os slots cheios; o tanque de lava no Heat Generator). Cada marca leva o conteúdo de cada slot, `getRedstoneLevel()` na hora, o
///    `currentRedstoneLevel` e a saída do comparador.
///
/// Geometria: a linha `r` fica em `z = 6r + 2`; N em (3, 1, z), S em (3, 1, z + 1), a fonte de S em (2, 1, z + 1) e os comparadores em (4, 1, z) e
/// (4, 1, z + 1), com a entrada a oeste (`FACING = WEST`).
@ForEachTest(groups = "trace")
public class ComparatorTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final int ROW_STRIDE = 6;

    private record Kind(String name, Supplier<BlockState> state) {
    }

    private static final Kind CHAMBER = new Kind("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.defaultState());
    private static final Kind CRUSHER = new Kind("crusher", () -> MekanismBlocks.CRUSHER.defaultState());
    private static final Kind SMELTER = new Kind("energized_smelter", () -> MekanismBlocks.ENERGIZED_SMELTER.defaultState());
    private static final Kind INFUSER = new Kind("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.defaultState());
    private static final Kind HEAT = new Kind("heat_generator", () -> GeneratorsBlocks.HEAT_GENERATOR.defaultState());

    private static BlockPos north(int row) {
        return new BlockPos(3, 1, ROW_STRIDE * row + 2);
    }

    private static BlockPos south(int row) {
        return new BlockPos(3, 1, ROW_STRIDE * row + 3);
    }

    private static BlockPos source(int row) {
        return new BlockPos(2, 1, ROW_STRIDE * row + 3);
    }

    private static BlockPos comparatorNorth(int row) {
        return new BlockPos(4, 1, ROW_STRIDE * row + 2);
    }

    private static BlockPos comparatorSouth(int row) {
        return new BlockPos(4, 1, ROW_STRIDE * row + 3);
    }

    private static BlockState comparator() {
        return Blocks.COMPARATOR.defaultBlockState().setValue(ComparatorBlock.FACING, Direction.WEST);
    }

    private static void template(DynamicTest test, int rows, boolean second) {
        test.registerGameTestTemplate(() -> {
            StructureTemplateBuilder builder = StructureTemplateBuilder.withSize(7, 3, ROW_STRIDE * rows + 1)
                  .fill(0, 0, 0, 6, 0, ROW_STRIDE * rows, Blocks.STONE.defaultBlockState());
            for (int row = 0; row < rows; row++) {
                BlockPos a = comparatorNorth(row);
                builder = builder.fill(a.getX(), a.getY(), a.getZ(), a.getX(), a.getY(), a.getZ(), comparator());
                if (second) {
                    BlockPos b = comparatorSouth(row);
                    builder = builder.fill(b.getX(), b.getY(), b.getZ(), b.getX(), b.getY(), b.getZ(), comparator());
                }
            }
            return builder;
        });
    }

    /// Um cenário: passos `(tick|pre ou step|operações separadas por ';')`. `pre` roda em `ServerTickEvent.Pre`, antes dos block entities; `step` no passo do gameTest.
    private static void script(DynamicTest test, Scene scene, int end, String... steps) {
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

    // ---- 1. o aviso do comparador dentro do tick da máquina ----

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_comparator")
    @TestHolder(description = "Records two Enrichment Chambers side by side, one with a stale redstone value, in the two ticker orders: the comparator change of one notifies the other inside its own tick.")
    public static void order(final DynamicTest test) {
        template(test, 2, true);
        // Linha 0: N entra antes de S (N tica primeiro). Linha 1: S entra antes de N (S tica primeiro).
        script(test, new Order("comparator_order", 2), 100,
              "5|step|put 0 N 3;put 0 S 3;put 1 S 3;put 1 N 3;mode 0 HIGH;mode 1 HIGH;fill 0;fill 1",
              "10|step|src 0 on 2;src 1 on 2",
              "20|step|ninput 0;ninput 1",
              "30|step|src 0 off 2;src 1 off 2",
              "40|step|nclear 0;nclear 1",
              "50|step|src 0 on 2;src 1 on 2",
              "55|pre|ninput 0;ninput 1",
              "70|step|clear 0 N 3;clear 1 N 3;src 0 off 2;src 1 off 2",
              "80|step|src 0 on 2;src 1 on 2;put 0 N 2;put 1 N 2");
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_comparator")
    @TestHolder(description = "Records a machine put without notifying its neighbours next to a machine with a stale redstone value: the first tick of the new machine.")
    public static void firstTick(final DynamicTest test) {
        template(test, 2, true);
        script(test, new Order("comparator_first_tick", 2), 60,
              "5|step|put 0 S 3;put 1 S 3;mode 0 HIGH;mode 1 HIGH;fill 0;fill 1",
              "10|step|src 0 on 2;src 1 on 2",
              "20|step|put 0 N 2;put 1 N 2",
              "30|step|src 0 off 2;src 1 off 2",
              "40|step|ninput 0;ninput 1");
    }

    private static final class Order extends Scene {
        Order(String name, int rows) {
            super(name, rows);
        }

        @Override
        void run(String op) {
            String[] words = op.split(" ");
            int row = Integer.parseInt(words[1]);
            var level = helper.getLevel();
            switch (words[0]) {
                case "put" -> level.setBlock(abs(words[2].equals("N") ? north(row) : south(row)), CHAMBER.state.get(), Integer.parseInt(words[3]));
                case "clear" -> level.setBlock(abs(words[2].equals("N") ? north(row) : south(row)), Blocks.AIR.defaultBlockState(), Integer.parseInt(words[3]));
                case "mode" -> tile(south(row)).setControlType(RedstoneControl.valueOf(words[2]));
                case "fill" -> fill(tile(south(row)));
                case "src" -> level.setBlock(abs(source(row)), words[2].equals("on") ? Blocks.REDSTONE_BLOCK.defaultBlockState() : Blocks.AIR.defaultBlockState(),
                      Integer.parseInt(words[3]));
                case "ninput" -> tile(north(row)).getInventorySlots().get(0).setContents(ItemResource.of(Items.CLAY), 64, null);
                case "nclear" -> tile(north(row)).getInventorySlots().get(0).setContents(ItemResource.EMPTY, 0, null);
                default -> throw new IllegalStateException(op);
            }
        }

        @Override
        String row(int row) {
            StringBuilder out = new StringBuilder("{");
            TileEntityMekanism s = tile(south(row));
            out.append("\"s\":");
            if (s == null) {
                out.append("null");
            } else {
                out.append("{\"powered\":").append(s.isPowered()).append(",\"can_function\":").append(s.canFunction()).append(",\"active\":").append(s.getActive())
                      .append(",\"comparator\":").append(s.getCurrentRedstoneLevel());
                if (s instanceof TileEntityProgressMachine<?> machine) {
                    out.append(",\"progress\":").append(machine.getRecipeProgress());
                }
                if (s.getEnergyContainer() instanceof MachineEnergyContainer<?> energy) {
                    out.append(",\"energy\":").append(energy.getAmountAsLong());
                }
                out.append('}');
            }
            TileEntityMekanism n = tile(north(row));
            out.append(",\"n\":");
            if (n == null) {
                out.append("null");
            } else {
                out.append("{\"comparator\":").append(n.getCurrentRedstoneLevel()).append(",\"level\":").append(n.getRedstoneLevel()).append('}');
            }
            out.append(",\"cn\":").append(output(comparatorNorth(row))).append(",\"cs\":").append(output(comparatorSouth(row)));
            return out.append('}').toString();
        }
    }

    // ---- 2. o valor analógico das cinco máquinas ----

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_comparator")
    @TestHolder(description = "Records the comparator reading of the Enrichment Chamber with empty, partly full and full slots.")
    public static void chamberLevels(final DynamicTest test) {
        levels(test, CHAMBER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_comparator")
    @TestHolder(description = "Records the comparator reading of the Crusher with empty, partly full and full slots.")
    public static void crusherLevels(final DynamicTest test) {
        levels(test, CRUSHER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_comparator")
    @TestHolder(description = "Records the comparator reading of the Energized Smelter with empty, partly full and full slots.")
    public static void smelterLevels(final DynamicTest test) {
        levels(test, SMELTER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_comparator")
    @TestHolder(description = "Records the comparator reading of the Metallurgic Infuser with empty, partly full and full slots.")
    public static void infuserLevels(final DynamicTest test) {
        levels(test, INFUSER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_comparator")
    @TestHolder(description = "Records the comparator reading of the Heat Generator with an empty, partly full and full lava tank.")
    public static void heatLevels(final DynamicTest test) {
        levels(test, HEAT);
    }

    private static void levels(DynamicTest test, Kind kind) {
        template(test, 1, false);
        Levels scene = new Levels("comparator_levels_" + kind.name, kind);
        String[] steps = kind == HEAT
              ? new String[]{
                    "5|step|put;mode HIGH",
                    "10|step|mark empty",
                    "11|step|lava 1",
                    "16|step|mark lava_1",
                    "17|step|lava 62",
                    "22|step|mark lava_62",
                    "23|step|lava 63",
                    "28|step|mark lava_63",
                    "29|step|lava 500",
                    "34|step|mark lava_500",
                    "35|step|lava 999",
                    "40|step|mark lava_999",
                    "41|step|lava 1000",
                    "46|step|mark lava_1000",
                    "47|step|lava 10000",
                    "52|step|mark lava_10000",
                    "53|step|lava 0",
                    "58|step|mark emptied",
                    "59|step|slots 64",
                    "64|step|mark fuel_64"}
              : new String[]{
                    "5|step|put",
                    "10|step|mark empty",
                    "11|step|slot 0 1",
                    "16|step|mark input_1",
                    "17|step|slot 0 32",
                    "22|step|mark input_32",
                    "23|step|slot 0 64",
                    "28|step|mark input_64",
                    "29|step|slot 1 64",
                    "34|step|mark input_and_output_64",
                    "35|step|all",
                    "40|step|mark all_full",
                    "41|step|none",
                    "46|step|mark emptied"};
        script(test, scene, 70, steps);
    }

    private static final class Levels extends Scene {
        private final Kind kind;
        private final List<String> marks = new ArrayList<>();

        Levels(String name, Kind kind) {
            super(name, 1);
            this.kind = kind;
        }

        private Item candidate(IInventorySlot slot) {
            for (Item item : List.of(Items.CLAY, Items.COBBLESTONE, Items.SAND, Items.DIRT, Items.COAL, Items.RAW_IRON, Items.COPPER_INGOT,
                  BuiltInRegistries.ITEM.getValue(Identifier.fromNamespaceAndPath("mekanism", "energy_tablet")),
                  BuiltInRegistries.ITEM.getValue(Identifier.fromNamespaceAndPath("mekanism", "upgrade")))) {
                if (item != Items.AIR && slot.isValid(ItemResource.of(item))) {
                    return item;
                }
            }
            return null;
        }

        @Override
        void run(String op) {
            String[] words = op.split(" ");
            var level = helper.getLevel();
            switch (words[0]) {
                case "put" -> level.setBlock(abs(north(0)), kind.state.get(), 3);
                case "mode" -> tile(north(0)).setControlType(RedstoneControl.valueOf(words[1]));
                case "lava" -> ((TileEntityHeatGenerator) tile(north(0))).lavaTank.setContents(
                      Integer.parseInt(words[1]) == 0 ? FluidResource.EMPTY : FluidResource.of(Fluids.LAVA), Integer.parseInt(words[1]), null);
                case "slot" -> {
                    int index = Integer.parseInt(words[1]) + (kind == INFUSER ? 1 : 0);
                    IInventorySlot slot = tile(north(0)).getInventorySlots().get(index);
                    Item item = candidate(slot);
                    slot.setContents(ItemResource.of(item), Integer.parseInt(words[2]), null);
                }
                case "slots" -> {
                    for (IInventorySlot slot : tile(north(0)).getInventorySlots()) {
                        Item item = candidate(slot);
                        if (item != null) {
                            slot.setContents(ItemResource.of(item), Integer.parseInt(words[1]), null);
                        }
                    }
                }
                case "all" -> {
                    for (IInventorySlot slot : tile(north(0)).getInventorySlots()) {
                        Item item = candidate(slot);
                        if (item != null) {
                            slot.setContents(ItemResource.of(item), slot.capacityAsLong(ItemResource.of(item)), null);
                        }
                    }
                }
                case "none" -> {
                    for (IInventorySlot slot : tile(north(0)).getInventorySlots()) {
                        slot.setContents(ItemResource.EMPTY, 0, null);
                    }
                }
                case "mark" -> {
                    TileEntityMekanism tile = tile(north(0));
                    StringBuilder out = new StringBuilder("{\"mark\":\"").append(words[1]).append("\",\"t\":").append(tick).append(",\"level\":").append(tile.getRedstoneLevel())
                          .append(",\"current\":").append(tile.getCurrentRedstoneLevel()).append(",\"slots\":[");
                    boolean first = true;
                    for (IInventorySlot slot : tile.getInventorySlots()) {
                        out.append(first ? "" : ",").append("{\"item\":\"").append(slot.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(slot.asItemStack().getItem()).getPath())
                              .append("\",\"count\":").append(slot.amountAsLong()).append(",\"capacity\":").append(slot.capacityAsLong(slot.resource())).append('}');
                        first = false;
                    }
                    out.append(']');
                    if (tile instanceof TileEntityHeatGenerator heat) {
                        out.append(",\"lava\":").append(heat.lavaTank.amountAsLong()).append(",\"lava_capacity\":").append(heat.lavaTank.capacityAsLong(heat.lavaTank.resource()));
                    }
                    marks.add(out.append('}').toString());
                }
                default -> throw new IllegalStateException(op);
            }
        }

        @Override
        String row(int row) {
            TileEntityMekanism tile = tile(north(0));
            if (tile == null) {
                return "null";
            }
            return "{\"level\":" + tile.getRedstoneLevel() + ",\"current\":" + tile.getCurrentRedstoneLevel() + ",\"cn\":" + output(comparatorNorth(0)) + "}";
        }

        @Override
        void write() {
            recording = false;
            List<String> all = new ArrayList<>();
            all.add("{\"machine\":\"" + kind.name + "\"}");
            all.addAll(marks);
            all.addAll(lines);
            ComparatorTraceTest.write(name, all);
        }
    }

    // ---- a cena em comum ----

    private abstract static class Scene {
        protected final String name;
        protected final int rows;
        protected final List<String> lines = new ArrayList<>();
        private final List<String> ops = new ArrayList<>();
        private final Map<Integer, List<String>> preOps = new HashMap<>();
        protected MekGameTestHelper helper;
        protected boolean recording;
        protected int tick;

        Scene(String name, int rows) {
            this.name = name;
            this.rows = rows;
        }

        void begin(MekGameTestHelper helper) {
            this.helper = helper;
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

        protected BlockPos abs(BlockPos pos) {
            return helper.absolutePos(pos);
        }

        protected TileEntityMekanism tile(BlockPos pos) {
            return helper.getLevel().getBlockEntity(abs(pos)) instanceof TileEntityMekanism tile ? tile : null;
        }

        protected int output(BlockPos pos) {
            return helper.getLevel().getBlockEntity(abs(pos)) instanceof ComparatorBlockEntity entity ? entity.getOutputSignal() : -1;
        }

        protected void fill(TileEntityMekanism tile) {
            if (tile instanceof TileEntityProgressMachine<?> machine) {
                machine.getInventorySlots().get(0).setContents(ItemResource.of(Items.CLAY), 64, null);
                if (tile.getEnergyContainer() instanceof MachineEnergyContainer<?> energy) {
                    energy.setEnergy(20_000, null);
                }
            }
        }

        void apply(String text, String phase) {
            for (String op : text.split(";")) {
                run(op.trim());
                ops.add("{\"op\":\"" + op.trim() + "\",\"phase\":\"" + phase + "\"}");
            }
        }

        abstract void run(String op);

        abstract String row(int row);

        void record() {
            if (!recording) {
                return;
            }
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++).append(",\"ops\":[");
            for (int i = 0; i < ops.size(); i++) {
                line.append(i == 0 ? "" : ",").append(ops.get(i));
            }
            ops.clear();
            line.append("],\"rows\":[");
            for (int row = 0; row < rows; row++) {
                line.append(row == 0 ? "" : ",").append(row(row));
            }
            lines.add(line.append("]}").toString());
        }

        void write() {
            recording = false;
            ComparatorTraceTest.write(name, lines);
        }
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
}
