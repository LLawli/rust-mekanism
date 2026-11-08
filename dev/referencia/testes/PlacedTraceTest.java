package mekanism.common.tests.network;

import it.unimi.dsi.fastutil.objects.Object2IntMap;
import it.unimi.dsi.fastutil.objects.Object2IntOpenHashMap;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Supplier;
import mekanism.api.AutomationType;
import mekanism.api.MekanismRegistries;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.resource.LargeResourceStack;
import mekanism.api.upgrade.Upgrade;
import mekanism.api.upgrade.UpgradeIds;
import mekanism.common.capabilities.energy.BasicEnergyContainer;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.component.component.UpgradeAware;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismDataComponents;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import mekanism.common.tile.prefab.TileEntityProgressMachine;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.tile.TileEntityGenerator;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava a capacidade de energia, o consumo e os ticks de cada máquina posta de dois jeitos: pelo gabarito de teste (`setBlock`, que nunca
/// passa por `applyImplicitComponents`) e por um jogador (`useOn` com o item, que passa), com e sem upgrades no item, e depois de
/// salvar o tile e criar outro do NBT (`BlockEntity.loadStatic`, o chunk que carrega: o filho `component_upgrade` só existe no NBT com upgrade ou item, e só ele chama `deserialize`). Também grava, tick a tick, uma máquina posta por um
/// jogador a que se dá uma só carga de energia, com a capacidade que ela tem: o consumo e o tempo de operação reais.
/// `placed_<máquina>.jsonl` (capacidade) e `placed_run_<máquina>_<variante>.jsonl` (a carga única).
@ForEachTest(groups = "trace")
public class PlacedTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final BlockPos TEMPLATE = new BlockPos(1, 1, 1);
    private static final BlockPos PLAYER_SPOT = new BlockPos(3, 1, 1);
    private static final BlockPos FLOOR = new BlockPos(3, 0, 1);
    /// Pares (velocidade, energia) postos no item. O Heat Generator só aceita o silêncio, então os pares dele usam só esse.
    private static final int[][] COMBOS = {{0, 0}, {1, 0}, {0, 1}, {4, 0}, {0, 4}, {8, 0}, {0, 8}, {3, 2}, {8, 8}};
    private static final int RUN_TICKS = 230;

    private record Spec(String name, Supplier<ItemLike> item, Supplier<BlockState> state, boolean upgradable, String input, String tank) {
    }

    private static final Spec CHAMBER = new Spec("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem(),
          () -> MekanismBlocks.ENRICHMENT_CHAMBER.defaultState(), true, "clay", null);
    private static final Spec CRUSHER = new Spec("crusher", () -> MekanismBlocks.CRUSHER.asItem(),
          () -> MekanismBlocks.CRUSHER.defaultState(), true, "cobblestone", null);
    private static final Spec SMELTER = new Spec("energized_smelter", () -> MekanismBlocks.ENERGIZED_SMELTER.asItem(),
          () -> MekanismBlocks.ENERGIZED_SMELTER.defaultState(), true, "sand", null);
    private static final Spec INFUSER = new Spec("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.asItem(),
          () -> MekanismBlocks.METALLURGIC_INFUSER.defaultState(), true, "dirt", "bio");
    private static final Spec HEAT = new Spec("heat_generator", () -> GeneratorsBlocks.HEAT_GENERATOR.asItem(),
          () -> GeneratorsBlocks.HEAT_GENERATOR.defaultState(), false, null, null);

    private static void template(DynamicTest test, Spec spec) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(6, 3, 3)
              .fill(3, 0, 1, 3, 0, 1, Blocks.STONE.defaultBlockState())
              .fill(1, 1, 1, 1, 1, 1, spec.state.get()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records the energy capacity of the Enrichment Chamber placed by template and by a player, with and without upgrades.")
    public static void chamberCapacity(final DynamicTest test) {
        capacity(test, CHAMBER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records the energy capacity of the Crusher placed by template and by a player, with and without upgrades.")
    public static void crusherCapacity(final DynamicTest test) {
        capacity(test, CRUSHER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records the energy capacity of the Energized Smelter placed by template and by a player, with and without upgrades.")
    public static void smelterCapacity(final DynamicTest test) {
        capacity(test, SMELTER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records the energy capacity of the Metallurgic Infuser placed by template and by a player, with and without upgrades.")
    public static void infuserCapacity(final DynamicTest test) {
        capacity(test, INFUSER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records the energy capacity of the Heat Generator placed by template and by a player, with and without upgrades.")
    public static void heatCapacity(final DynamicTest test) {
        capacity(test, HEAT);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records an Enrichment Chamber placed by a player and given one burst of energy, plain and with speed 2.")
    public static void chamberPlacedRun(final DynamicTest test) {
        run(test, CHAMBER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records a Crusher placed by a player and given one burst of energy, plain and with speed 2.")
    public static void crusherPlacedRun(final DynamicTest test) {
        run(test, CRUSHER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records an Energized Smelter placed by a player and given one burst of energy, plain and with speed 2.")
    public static void smelterPlacedRun(final DynamicTest test) {
        run(test, SMELTER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records a Metallurgic Infuser placed by a player and given one burst of energy, plain and with speed 2.")
    public static void infuserPlacedRun(final DynamicTest test) {
        run(test, INFUSER);
    }


    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records the Enrichment Chamber placed by a player next to a full Heat Generator: the capacity and the energy in the tick of the placement and the next ones.")
    public static void chamberFed(final DynamicTest test) {
        fed(test, CHAMBER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records the Crusher placed by a player next to a full Heat Generator: the capacity and the energy in the tick of the placement and the next ones.")
    public static void crusherFed(final DynamicTest test) {
        fed(test, CRUSHER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records the Energized Smelter placed by a player next to a full Heat Generator: the capacity and the energy in the tick of the placement and the next ones.")
    public static void smelterFed(final DynamicTest test) {
        fed(test, SMELTER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_placed")
    @TestHolder(description = "Records the Metallurgic Infuser placed by a player next to a full Heat Generator: the capacity and the energy in the tick of the placement and the next ones.")
    public static void infuserFed(final DynamicTest test) {
        fed(test, INFUSER);
    }

    /// A Heat Generator cheio (240) encostado, de frente para a máquina (`ENERGY_SIDES` é só a frente), e a máquina posta por um jogador em dois momentos do tick:
    /// `pre` (`ServerTickEvent.Pre`, antes dos block entities, como uma ação do jogador, que roda antes do tick: o gerador tica depois, no mesmo tick) e `step` (depois dos
    /// block entities: o gerador só empurra no tick seguinte).
    private static void fed(DynamicTest test, Spec spec) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(6, 3, 3)
              .fill(0, 0, 0, 5, 0, 2, Blocks.STONE.defaultBlockState())
              .fill(1, 1, 1, 1, 1, 1, GeneratorsBlocks.HEAT_GENERATOR.defaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, Direction.EAST)));
        Fed scene = new Fed(spec);
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> scene.pre());
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> scene.begin(helper));
            int tick = 0;
            for (String phase : new String[]{"pre", "step"}) {
                for (int[] combo : FED_COMBOS) {
                    int at = scene.nextSlot();
                    if (phase.equals("pre")) {
                        scene.schedulePre(at, phase, combo);
                    } else {
                        sequence.thenExecuteAfter(at - tick, () -> scene.place(phase, combo));
                        tick = at;
                    }
                    sequence.thenExecuteAfter(Math.max(0, at + FED_SPAN - tick), () -> scene.clear());
                    tick = Math.max(tick, at + FED_SPAN);
                }
            }
            sequence.thenExecuteAfter(2, scene::finish);
            sequence.thenSucceed();
        });
    }

    private static final int[][] FED_COMBOS = {{0, 0}, {2, 0}, {0, 4}, {3, 2}, {8, 8}};
    private static final int FED_SPAN = 8;
    private static final BlockPos FED_SPOT = new BlockPos(2, 1, 1);
    private static final BlockPos FED_FLOOR = new BlockPos(2, 0, 1);
    private static final BlockPos FED_GENERATOR = new BlockPos(1, 1, 1);

    private static final class Fed {
        private final Spec spec;
        private final List<String> lines = new ArrayList<>();
        private final java.util.Map<Integer, Object[]> pre = new java.util.HashMap<>();
        private MekGameTestHelper helper;
        private Player player;
        private boolean recording;
        private String label = "";
        private int since = -1;
        private int tick;
        private int slots;

        Fed(Spec spec) {
            this.spec = spec;
        }

        void begin(MekGameTestHelper helper) {
            this.helper = helper;
            player = helper.makeMockPlayerLookingAt(FED_FLOOR, Direction.UP);
            recording = true;
        }

        /// O próximo tick livre, 12 depois do anterior (a primeira marca fica no tick 10, depois dos ticks de arranque).
        int nextSlot() {
            return 10 + (FED_SPAN + 4) * slots++;
        }

        void schedulePre(int at, String phase, int[] combo) {
            pre.put(at, new Object[]{phase, combo});
        }

        void pre() {
            if (!recording) {
                return;
            }
            Object[] entry = pre.get(tick);
            if (entry != null) {
                place((String) entry[0], (int[]) entry[1]);
            }
        }

        void place(String phase, int[] combo) {
            helper.setBlock(FED_SPOT, Blocks.AIR.defaultBlockState());
            generator().energyContainer().setEnergy(240, null);
            helper.useOn(FED_FLOOR, upgraded(helper, spec, combo[0], combo[1], 0), player, Direction.UP);
            label = phase + "_" + combo[0] + "_" + combo[1];
            since = 0;
            lines.add("{\"placed\":\"" + label + "\",\"t\":" + tick + ",\"phase\":\"" + phase + "\",\"speed\":" + combo[0] + ",\"energy_upgrades\":" + combo[1]
                  + ",\"at_placement\":" + state() + "}");
        }

        void clear() {
            helper.setBlock(FED_SPOT, Blocks.AIR.defaultBlockState());
            since = -1;
        }

        private TileEntityHeatGenerator generator() {
            return (TileEntityHeatGenerator) helper.getLevel().getBlockEntity(helper.absolutePos(FED_GENERATOR));
        }

        private String state() {
            BlockEntity entity = helper.getLevel().getBlockEntity(helper.absolutePos(FED_SPOT));
            StringBuilder out = new StringBuilder("{\"generator\":").append(generator().energyContainer().getAmountAsLong());
            if (entity instanceof TileEntityProgressMachine<?> machine && machine.getEnergyContainer() instanceof MachineEnergyContainer<?> energy) {
                out.append(",\"energy\":").append(energy.getAmountAsLong()).append(",\"max_energy\":").append(currentMaxEnergy(energy)).append(",\"capacity\":")
                      .append(energy.getCapacityAsLong());
            }
            return out.append('}').toString();
        }

        void record() {
            if (!recording) {
                return;
            }
            if (since >= 0) {
                lines.add("{\"placed\":\"" + label + "\",\"t\":" + tick + ",\"since\":" + since++ + ",\"after_tick\":" + state() + "}");
            }
            tick++;
        }

        void finish() {
            recording = false;
            write("placed_fed_" + spec.name, lines);
        }
    }

    private static void capacity(DynamicTest test, Spec spec) {
        template(test, spec);
        Capacity scene = new Capacity(spec);
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> scene.begin(helper));
            sequence.thenExecuteAfter(1, () -> scene.recordAt(TEMPLATE, "template", 0, 0));
            sequence.thenExecuteAfter(1, () -> scene.recordLoaded(TEMPLATE, "template_loaded", 0, 0));
            for (int[] combo : COMBOS) {
                if (!spec.upgradable && (combo[0] != 0 || combo[1] != 0)) {
                    continue;
                }
                sequence.thenExecuteAfter(1, () -> scene.placeByPlayer(combo[0], combo[1]));
                sequence.thenExecuteAfter(1, () -> scene.recordAt(PLAYER_SPOT, "use_on", combo[0], combo[1]));
                sequence.thenExecuteAfter(1, () -> scene.recordLoaded(PLAYER_SPOT, "use_on_loaded", combo[0], combo[1]));
            }
            if (!spec.upgradable) {
                sequence.thenExecuteAfter(1, () -> scene.placeByPlayerMuffling());
                sequence.thenExecuteAfter(1, () -> scene.recordAt(PLAYER_SPOT, "use_on_muffling", 0, 0));
                sequence.thenExecuteAfter(1, () -> scene.recordLoaded(PLAYER_SPOT, "use_on_muffling_loaded", 0, 0));
            }
            sequence.thenExecuteAfter(1, scene::finish);
            sequence.thenSucceed();
        });
    }

    private static Holder<Upgrade> upgrade(MekGameTestHelper helper, net.minecraft.resources.ResourceKey<Upgrade> key) {
        return helper.getLevel().registryAccess().lookupOrThrow(MekanismRegistries.Keys.UPGRADES).getOrThrow(key);
    }

    private static ItemStack upgraded(MekGameTestHelper helper, Spec spec, int speed, int energy, int muffling) {
        ItemStack stack = new ItemStack(spec.item.get());
        Object2IntMap<Holder<Upgrade>> map = new Object2IntOpenHashMap<>();
        if (speed > 0) {
            map.put(upgrade(helper, UpgradeIds.SPEED), speed);
        }
        if (energy > 0) {
            map.put(upgrade(helper, UpgradeIds.ENERGY), energy);
        }
        if (muffling > 0) {
            map.put(upgrade(helper, UpgradeIds.MUFFLING), muffling);
        }
        if (!map.isEmpty()) {
            stack.set(MekanismDataComponents.UPGRADES, new UpgradeAware(map, LargeResourceStack.ITEM_HELPER.empty(), LargeResourceStack.ITEM_HELPER.empty()));
        }
        return stack;
    }

    /// `currentMaxEnergy` do container, privado: o getter público devolve o maior entre ele e o que está guardado.
    private static long currentMaxEnergy(MachineEnergyContainer<?> energy) {
        try {
            Field field = MachineEnergyContainer.class.getDeclaredField("currentMaxEnergy");
            field.setAccessible(true);
            return field.getLong(energy);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
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

    private static Item item(String id) {
        return BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(id));
    }

    private static final class Capacity {
        private final Spec spec;
        private final List<String> lines = new ArrayList<>();
        private MekGameTestHelper helper;
        private Player player;

        Capacity(Spec spec) {
            this.spec = spec;
        }

        void begin(MekGameTestHelper helper) {
            this.helper = helper;
            player = helper.makeMockPlayerLookingAt(FLOOR, Direction.UP);
        }

        void placeByPlayer(int speed, int energy) {
            clear(PLAYER_SPOT);
            helper.useOn(FLOOR, upgraded(helper, spec, speed, energy, 0), player, Direction.UP);
        }

        void placeByPlayerMuffling() {
            clear(PLAYER_SPOT);
            helper.useOn(FLOOR, upgraded(helper, spec, 0, 0, 1), player, Direction.UP);
        }

        private void clear(BlockPos pos) {
            helper.setBlock(pos, Blocks.AIR.defaultBlockState());
        }

        TileEntityMekanism tile(BlockPos pos) {
            BlockEntity tile = helper.getLevel().getBlockEntity(helper.absolutePos(pos));
            return (TileEntityMekanism) tile;
        }

        /// O que o jogo faz ao carregar o chunk: grava o tile e cria outro objeto a partir do NBT (`BlockEntity.loadStatic`, que roda o
        /// construtor e o `loadAdditional` com o jogo sem nível ainda). A capacidade que ele tem é a de um tile que acabou de carregar.
        void recordLoaded(BlockPos pos, String how, int speed, int energy) {
            TileEntityMekanism tile = tile(pos);
            CompoundTag tag = tile.saveWithFullMetadata(helper.getLevel().registryAccess());
            BlockEntity loaded = BlockEntity.loadStatic(helper.absolutePos(pos), helper.getLevel().getBlockState(helper.absolutePos(pos)), tag,
                  helper.getLevel().registryAccess());
            if (!(loaded instanceof TileEntityMekanism fresh) || fresh == tile) {
                throw new IllegalStateException("loadStatic gave " + loaded);
            }
            List<String> keys = new ArrayList<>(tag.keySet());
            java.util.Collections.sort(keys);
            String upgradeTag = tag.contains("component_upgrade") ? tag.get("component_upgrade").toString().replace("\\", "\\\\").replace("\"", "\\\"") : "";
            record(fresh, how, speed, energy, ",\"saved_keys\":" + keys.stream().map(key -> "\"" + key + "\"").toList()
                  + ",\"component_upgrade\":\"" + upgradeTag + "\"");
        }

        void recordAt(BlockPos pos, String how, int speed, int energy) {
            record(tile(pos), how, speed, energy, "");
        }

        private void record(TileEntityMekanism tile, String how, int speed, int energy, String extra) {
            StringBuilder line = new StringBuilder("{\"machine\":\"").append(spec.name).append("\",\"how\":\"").append(how)
                  .append("\",\"speed\":").append(speed).append(",\"energy\":").append(energy);
            if (tile.getEnergyContainer() instanceof MachineEnergyContainer<?> machine) {
                line.append(",\"max_energy\":").append(currentMaxEnergy(machine)).append(",\"capacity_getter\":").append(machine.getCapacityAsLong())
                      .append(",\"base_max_energy\":").append(machine.getBaseMaxEnergy()).append(",\"energy_per_tick\":").append(machine.getEnergyPerTick());
                if (tile instanceof TileEntityProgressMachine<?> progress) {
                    line.append(",\"ticks_required\":").append(progress.getTicksRequired());
                }
            } else if (tile instanceof TileEntityGenerator generator) {
                BasicEnergyContainer container = generator.energyContainer();
                line.append(",\"max_energy\":").append(container.getCapacityAsLong()).append(",\"capacity_getter\":").append(container.getCapacityAsLong());
            }
            lines.add(line.append(extra).append('}').toString());
        }

        void finish() {
            write("placed_" + spec.name, lines);
        }
    }

    private static void run(DynamicTest test, Spec spec) {
        template(test, spec);
        Run scene = new Run(spec);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> scene.begin(helper));
            sequence.thenExecuteAfter(1, () -> scene.start("plain", 0));
            sequence.thenExecuteAfter(3, scene::burst);
            sequence.thenExecuteAfter(RUN_TICKS, () -> scene.stop());
            sequence.thenExecuteAfter(1, () -> scene.start("speed2", 2));
            sequence.thenExecuteAfter(3, scene::burst);
            sequence.thenExecuteAfter(RUN_TICKS, () -> scene.stop());
            sequence.thenExecuteAfter(1, scene::finish);
            sequence.thenSucceed();
        });
    }

    /// Uma máquina posta por um jogador, com uma só carga de energia: depois dela não entra mais nada.
    private static final class Run {
        private final Spec spec;
        private final List<String> lines = new ArrayList<>();
        private MekGameTestHelper helper;
        private Player player;
        private String variant = "";
        private boolean recording;
        private int tick;
        private final List<String> files = new ArrayList<>();
        private final List<List<String>> contents = new ArrayList<>();

        Run(Spec spec) {
            this.spec = spec;
        }

        void begin(MekGameTestHelper helper) {
            this.helper = helper;
            player = helper.makeMockPlayerLookingAt(FLOOR, Direction.UP);
        }

        TileEntityProgressMachine<?> tile() {
            return (TileEntityProgressMachine<?>) helper.getLevel().getBlockEntity(helper.absolutePos(PLAYER_SPOT));
        }

        MachineEnergyContainer<?> energy() {
            return (MachineEnergyContainer<?>) tile().getEnergyContainer();
        }

        void start(String variant, int speed) {
            this.variant = variant;
            helper.setBlock(PLAYER_SPOT, Blocks.AIR.defaultBlockState());
            helper.useOn(FLOOR, upgraded(helper, spec, speed, 0, 0), player, Direction.UP);
            lines.clear();
            tick = 0;
            int inputSlot = spec.tank == null ? 0 : 1;
            tile().getInventorySlots().get(inputSlot).setContents(ItemResource.of(item(spec.input)), 5, null);
            if (spec.tank != null) {
                Holder<Chemical> bio = helper.getLevel().registryAccess().getOrThrow(ChemicalIds.BIO);
                ((TileEntityMetallurgicInfuser) tile()).infusionTank.setContents(ChemicalResource.of(bio), 500, null);
            }
            lines.add("{\"machine\":\"" + spec.name + "\",\"variant\":\"" + variant + "\",\"speed\":" + speed + ",\"max_energy\":"
                  + currentMaxEnergy(energy()) + ",\"energy_per_tick\":" + energy().getEnergyPerTick() + ",\"ticks_required\":" + tile().getTicksRequired() + "}");
            recording = true;
        }

        /// Uma só carga, a que cabe: o que um cabo faria com o que a máquina aceita.
        void burst() {
            long accepted;
            try (Transaction transaction = Transaction.openRoot()) {
                accepted = energy().insert(Integer.MAX_VALUE, transaction, AutomationType.EXTERNAL);
                transaction.commit();
            }
            lines.add("{\"burst\":" + accepted + "}");
        }

        void record() {
            if (!recording || variant.isEmpty()) {
                return;
            }
            TileEntityProgressMachine<?> tile = tile();
            int outputSlot = spec.tank == null ? 1 : 2;
            ItemStack output = tile.getInventorySlots().get(outputSlot).asItemStack();
            lines.add("{\"t\":" + tick++ + ",\"energy\":" + energy().getAmountAsLong() + ",\"progress\":" + tile.getRecipeProgress() + ",\"required\":"
                  + tile.getTicksRequired() + ",\"per_tick\":" + energy().getEnergyPerTick() + ",\"output\":[\""
                  + (output.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(output.getItem()).getPath()) + "\"," + output.getCount() + "]}");
        }

        void stop() {
            recording = false;
            files.add("placed_run_" + spec.name + "_" + variant);
            contents.add(new ArrayList<>(lines));
        }

        void finish() {
            for (int i = 0; i < files.size(); i++) {
                write(files.get(i), contents.get(i));
            }
        }
    }
}
