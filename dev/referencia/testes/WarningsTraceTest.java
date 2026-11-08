package mekanism.common.tests.network;

import it.unimi.dsi.fastutil.objects.Object2IntMap;
import it.unimi.dsi.fastutil.objects.Object2IntOpenHashMap;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;
import java.util.function.BooleanSupplier;
import java.util.function.Supplier;
import mekanism.api.MekanismRegistries;
import mekanism.api.SerializationConstants;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.inventory.IInventorySlot;
import mekanism.api.recipes.ItemStackChemicalToItemStackRecipe;
import mekanism.api.recipes.ItemStackToItemStackRecipe;
import mekanism.api.recipes.cache.CachedRecipe.OperationTracker.RecipeError;
import mekanism.api.resource.LargeResourceStack;
import mekanism.api.upgrade.Upgrade;
import mekanism.api.upgrade.UpgradeIds;
import mekanism.common.block.attribute.Attribute;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.component.component.UpgradeAware;
import mekanism.common.component.containers.type.ContainerType;
import mekanism.common.inventory.warning.WarningTracker;
import mekanism.common.inventory.warning.WarningTracker.WarningType;
import mekanism.common.recipe.MekanismRecipeType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismDataComponents;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.interfaces.IRedstoneControl.RedstoneControl;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import mekanism.common.tile.prefab.TileEntityProgressMachine;
import mekanism.common.tile.prefab.TileEntityRecipeMachine;
import mekanism.generators.common.registries.GeneratorsBlocks;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava os avisos das quatro máquinas portadas: o que o `NOT_ENOUGH_INPUT` e o `NOT_ENOUGH_SECONDARY_INPUT` fazem (nada: a entrada que não basta
/// é "receita que não casa", `mismatchedRecipe`, sem erro), o overlay de nível >= 0,98 da barra de energia (só o erro velho de uma máquina pausada o
/// alcança) e o PULSE, que nenhuma das cinco máquinas aceita (`ITileRedstone.supportsMode`). `warnings_<cenário>.jsonl`, `warnings_sweep_<máquina>.jsonl`
/// e `redstone_pulse.jsonl`. A máquina é posta por um jogador (`useOn` com o item, que passa por `applyImplicitComponents`).
@ForEachTest(groups = "trace")
public class WarningsTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final int SWEEP_TIMEOUT = 600 * SharedConstants.TICKS_PER_SECOND;
    private static final BlockPos FLOOR = new BlockPos(3, 0, 1);
    private static final BlockPos SPOT = new BlockPos(3, 1, 1);
    private static final double OVERLAY_LEVEL = 0.98;

    private static final List<RecipeError> ERRORS = List.of(RecipeError.NOT_ENOUGH_ENERGY, RecipeError.NOT_ENOUGH_INPUT,
          RecipeError.NOT_ENOUGH_OUTPUT_SPACE, RecipeError.INPUT_DOESNT_PRODUCE_OUTPUT, RecipeError.NOT_ENOUGH_SECONDARY_INPUT);
    private static final List<String> ERROR_NAMES = List.of("energy", "input", "output_space", "wrong_output", "secondary");

    private enum Kind {
        CHAMBER("enrichment_chamber", 0, 1, false),
        CRUSHER("crusher", 0, 1, false),
        SMELTER("energized_smelter", 0, 1, false),
        INFUSER("metallurgic_infuser", 1, 2, true);

        final String id;
        final int input;
        final int output;
        final boolean chemical;

        Kind(String id, int input, int output, boolean chemical) {
            this.id = id;
            this.input = input;
            this.output = output;
            this.chemical = chemical;
        }

        Item item() {
            return switch (this) {
                case CHAMBER -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem();
                case CRUSHER -> MekanismBlocks.CRUSHER.asItem();
                case SMELTER -> MekanismBlocks.ENERGIZED_SMELTER.asItem();
                case INFUSER -> MekanismBlocks.METALLURGIC_INFUSER.asItem();
            };
        }
    }

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(6, 3, 3).fill(3, 0, 1, 3, 0, 1, Blocks.STONE.defaultBlockState()));
    }

    private static Item item(String id) {
        return BuiltInRegistries.ITEM.getValue(net.minecraft.resources.Identifier.parse(id));
    }

    // ---------------------------------------------------------------------------------------------------------------- cenários tick a tick

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_warnings")
    @TestHolder(description = "Records the Enrichment Chamber with a player in the GUI and less input than the recipe asks for (3 raw iron), arriving, leaving in the middle of the operation, and with speed upgrades.")
    public static void warningsInputChamber(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("warnings_input_chamber", Kind.CHAMBER);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper, 0, 0))
              .thenExecuteAfter(5, () -> scene.apply("raw_iron 2, energy 20000, player opens"))
              .thenExecuteAfter(10, () -> scene.apply("raw_iron 1"))
              .thenExecuteAfter(10, () -> scene.apply("raw_iron 3"))
              .thenExecuteAfter(25, () -> scene.apply("raw_iron 2"))
              .thenExecuteAfter(10, () -> scene.apply("raw_iron 3"))
              .thenExecuteAfter(25, () -> scene.apply("remove input"))
              .thenExecuteAfter(10, () -> scene.apply("raw_iron 5"))
              .thenExecuteAfter(30, () -> scene.apply("raw_iron 64, speed 8, energy topped up to 30 ticks every tick"))
              .thenExecuteAfter(40, () -> scene.apply("raw_iron 2"))
              .thenExecuteAfter(15, () -> scene.apply("raw_iron 4"))
              .thenExecuteAfter(30, () -> scene.apply("player closes"))
              .thenExecuteAfter(10, () -> scene.apply("raw_iron 2"))
              .thenExecuteAfter(15, () -> scene.apply("raw_iron 3"))
              .thenExecuteAfter(30, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_warnings")
    @TestHolder(description = "Records the Metallurgic Infuser with a player in the GUI and less chemical than the recipe asks for, arriving, leaving in the middle of the operation, and with a recipe that asks for 3 items.")
    public static void warningsInputInfuser(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("warnings_input_infuser", Kind.INFUSER);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper, 0, 0))
              .thenExecuteAfter(5, () -> scene.apply("item copper_ingot 2, tank redstone 9, energy 20000, player opens"))
              .thenExecuteAfter(10, () -> scene.apply("tank redstone 10"))
              .thenExecuteAfter(30, () -> scene.apply("tank redstone 9"))
              .thenExecuteAfter(10, () -> scene.apply("tank redstone 100"))
              .thenExecuteAfter(30, () -> scene.apply("item copper_ingot 0"))
              .thenExecuteAfter(10, () -> scene.apply("item copper_ingot 1"))
              .thenExecuteAfter(30, () -> scene.apply("tank redstone 0"))
              .thenExecuteAfter(10, () -> scene.apply("tank redstone 1000"))
              .thenExecuteAfter(30, () -> scene.apply("item iron_ingot 2, tank redstone 5"))
              .thenExecuteAfter(15, () -> scene.apply("tank redstone 1000, speed 8, energy topped up to 30 ticks every tick"))
              .thenExecuteAfter(40, () -> scene.apply("tank redstone 12"))
              .thenExecuteAfter(15, () -> scene.apply("tank redstone 25, item copper_ingot 2"))
              .thenExecuteAfter(30, () -> scene.apply("player closes"))
              .thenExecuteAfter(10, () -> scene.apply("tank redstone 9"))
              .thenExecuteAfter(15, () -> scene.apply("tank redstone 10"))
              .thenExecuteAfter(30, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_warnings")
    @TestHolder(description = "Records the energy bar of a paused Enrichment Chamber: the error that stays after the speed upgrades go away leaves the energy above the capacity, which is the bar warning overlay (level >= 0.98).")
    public static void warningsOverlay(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("warnings_overlay", Kind.CHAMBER);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper, 0, 0))
              .thenExecuteAfter(5, () -> scene.apply("clay 64, speed 8, energy 400, player opens"))
              .thenExecuteAfter(20, () -> scene.apply("remove speed all"))
              .thenExecuteAfter(20, () -> scene.apply("energy 21"))
              .thenExecuteAfter(20, () -> scene.apply("energy 2"))
              .thenExecuteAfter(20, () -> scene.apply("speed 8"))
              .thenExecuteAfter(20, () -> scene.apply("energy 9999"))
              .thenExecuteAfter(20, () -> scene.apply("energy 3"))
              .thenExecuteAfter(20, () -> scene.apply("remove speed"))
              .thenExecuteAfter(20, () -> scene.apply("remove speed"))
              .thenExecuteAfter(20, () -> scene.apply("energy 4"))
              .thenExecuteAfter(20, () -> scene.apply("energy 400"))
              .thenExecuteAfter(20, () -> scene.apply("energy 4"))
              .thenExecuteAfter(20, scene::write)
              .thenSucceed());
    }

    // ---------------------------------------------------------------------------------------------------------------- varredura de receitas

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = SWEEP_TIMEOUT, batch = "trace_warnings_sweep")
    @TestHolder(description = "Records every Enrichment Chamber recipe with the input count below, at and above what it asks for.")
    public static void warningsSweepChamber(final DynamicTest test) {
        sweep(test, Kind.CHAMBER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = SWEEP_TIMEOUT, batch = "trace_warnings_sweep")
    @TestHolder(description = "Records every Crusher recipe with the input count below, at and above what it asks for.")
    public static void warningsSweepCrusher(final DynamicTest test) {
        sweep(test, Kind.CRUSHER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = SWEEP_TIMEOUT, batch = "trace_warnings_sweep")
    @TestHolder(description = "Records every Energized Smelter recipe with the input count below, at and above what it asks for.")
    public static void warningsSweepSmelter(final DynamicTest test) {
        sweep(test, Kind.SMELTER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = SWEEP_TIMEOUT, batch = "trace_warnings_sweep")
    @TestHolder(description = "Records every Metallurgic Infuser recipe with the item count and the tank amount below, at and above what it asks for.")
    public static void warningsSweepInfuser(final DynamicTest test) {
        sweep(test, Kind.INFUSER);
    }

    private static void sweep(DynamicTest test, Kind kind) {
        template(test);
        Sweep sweep = new Sweep(kind);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> sweep.onTick());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> sweep.begin(helper))
              .thenWaitUntil(() -> {
                  if (!sweep.finished()) {
                      helper.fail("sweep still running");
                  }
              })
              .thenExecute(sweep::write)
              .thenSucceed());
    }

    // ---------------------------------------------------------------------------------------------------------------- PULSE

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_warnings_pulse")
    @TestHolder(description = "Records setControlType(PULSE), the saved tag and the item component with PULSE on the four machines and the Heat Generator: none accepts it.")
    public static void redstonePulse(final DynamicTest test) {
        template(test);
        List<String> lines = new ArrayList<>();
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  List<Supplier<ItemStack>> machines = List.of(() -> new ItemStack(Kind.CHAMBER.item()), () -> new ItemStack(Kind.CRUSHER.item()),
                        () -> new ItemStack(Kind.SMELTER.item()), () -> new ItemStack(Kind.INFUSER.item()),
                        () -> new ItemStack(GeneratorsBlocks.HEAT_GENERATOR.asItem()));
                  List<String> names = List.of("enrichment_chamber", "crusher", "energized_smelter", "metallurgic_infuser", "heat_generator");
                  for (int i = 0; i < machines.size(); i++) {
                      pulse(helper, names.get(i), machines.get(i), lines);
                  }
                  write("redstone_pulse", lines);
              })
              .thenSucceed());
    }

    private static void pulse(MekGameTestHelper helper, String name, Supplier<ItemStack> stack, List<String> lines) {
        Player player = helper.makeMockPlayerLookingAt(FLOOR, Direction.UP);
        BlockPos pos = helper.absolutePos(SPOT);
        helper.getLevel().destroyBlock(pos, false);
        helper.useOn(FLOOR, stack.get(), player, Direction.UP);
        TileEntityMekanism tile = (TileEntityMekanism) helper.getLevel().getBlockEntity(pos);
        lines.add(pulseLine(name, "placed", "", tile));
        for (RedstoneControl start : new RedstoneControl[]{RedstoneControl.DISABLED, RedstoneControl.HIGH, RedstoneControl.LOW}) {
            tile.setControlType(start);
            String before = tile.getControlType().name().toLowerCase();
            tile.setControlType(RedstoneControl.PULSE);
            lines.add(pulseLine(name, "set", before, tile));
        }
        for (RedstoneControl start : new RedstoneControl[]{RedstoneControl.DISABLED, RedstoneControl.HIGH, RedstoneControl.LOW}) {
            tile.setControlType(start);
            String before = tile.getControlType().name().toLowerCase();
            CompoundTag tag = tile.saveWithFullMetadata(helper.getLevel().registryAccess());
            tag.putString(SerializationConstants.CONTROL_TYPE, "pulse");
            BlockEntity loaded = BlockEntity.loadStatic(pos, helper.getLevel().getBlockState(pos), tag, helper.getLevel().registryAccess());
            if (!(loaded instanceof TileEntityMekanism loadedTile)) {
                throw new IllegalStateException("loadStatic gave " + loaded);
            }
            lines.add(pulseLine(name, "tag", before, loadedTile));
        }
        helper.getLevel().destroyBlock(pos, false);
        ItemStack withComponent = stack.get();
        withComponent.set(MekanismDataComponents.REDSTONE_CONTROL, RedstoneControl.PULSE);
        helper.useOn(FLOOR, withComponent, player, Direction.UP);
        TileEntityMekanism placed = (TileEntityMekanism) helper.getLevel().getBlockEntity(pos);
        lines.add(pulseLine(name, "component", "pulse", placed));
        helper.getLevel().destroyBlock(pos, false);
    }

    private static String pulseLine(String name, String route, String before, TileEntityMekanism tile) {
        return "{\"machine\":\"" + name + "\",\"route\":\"" + route + "\",\"before\":\"" + before + "\",\"supports_redstone\":" + tile.supportsRedstone()
              + ",\"supports_pulse\":" + tile.supportsMode(RedstoneControl.PULSE) + ",\"supports_high\":" + tile.supportsMode(RedstoneControl.HIGH)
              + ",\"mode\":\"" + tile.getControlType().name().toLowerCase() + "\"}";
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

    // ---------------------------------------------------------------------------------------------------------------- a máquina em cena

    private static class Machine {
        final Kind kind;
        MekGameTestHelper helper;
        Player player;

        Machine(Kind kind) {
            this.kind = kind;
        }

        void place(MekGameTestHelper helper, int speed, int energy) {
            this.helper = helper;
            player = helper.makeMockPlayerLookingAt(FLOOR, Direction.UP);
            helper.useOn(FLOOR, upgraded(speed, energy), player, Direction.UP);
        }

        Holder<Upgrade> upgrade(ResourceKey<Upgrade> key) {
            return helper.getLevel().registryAccess().lookupOrThrow(MekanismRegistries.Keys.UPGRADES).getOrThrow(key);
        }

        ItemStack upgraded(int speed, int energy) {
            ItemStack stack = new ItemStack(kind.item());
            Object2IntMap<Holder<Upgrade>> map = new Object2IntOpenHashMap<>();
            if (speed > 0) {
                map.put(upgrade(UpgradeIds.SPEED), speed);
            }
            if (energy > 0) {
                map.put(upgrade(UpgradeIds.ENERGY), energy);
            }
            if (!map.isEmpty()) {
                stack.set(MekanismDataComponents.UPGRADES, new UpgradeAware(map, LargeResourceStack.ITEM_HELPER.empty(), LargeResourceStack.ITEM_HELPER.empty()));
            }
            return stack;
        }

        TileEntityRecipeMachine<?, ?> tile() {
            BlockEntity tile = helper.getLevel().getBlockEntity(helper.absolutePos(SPOT));
            return tile instanceof TileEntityRecipeMachine<?, ?> machine ? machine : null;
        }

        TileEntityProgressMachine<?> machine() {
            return (TileEntityProgressMachine<?>) tile();
        }

        MachineEnergyContainer<?> energy() {
            return (MachineEnergyContainer<?>) tile().getEnergyContainer();
        }

        IInventorySlot slot(int index) {
            return tile().getInventorySlots().get(index);
        }

        void setInput(Item item, int count) {
            slot(kind.input).setContents(count == 0 ? ItemResource.EMPTY : ItemResource.of(item), count, null);
        }

        void setTank(Holder<Chemical> chemical, long amount) {
            ((TileEntityMetallurgicInfuser) tile()).infusionTank.setContents(amount == 0 ? ChemicalResource.EMPTY : ChemicalResource.of(chemical), amount, null);
        }

        void setTank(String chemical, long amount) {
            setTank(helper.getLevel().registryAccess().getOrThrow(chemical.equals("bio") ? ChemicalIds.BIO : ChemicalIds.REDSTONE), amount);
        }

        void setUpgrades(ResourceKey<Upgrade> key, int total) {
            var component = tile().getComponent();
            Holder<Upgrade> upgrade = upgrade(key);
            int installed = component.getUpgrades(upgrade);
            if (total > installed) {
                component.addUpgrades(helper.getLevel().registryAccess(), upgrade, total - installed);
            }
            while (component.getUpgrades(upgrade) > total) {
                component.removeUpgrade(helper.getLevel().registryAccess(), upgrade, false);
            }
        }

        void removeAll(ResourceKey<Upgrade> key) {
            tile().getComponent().removeUpgrade(helper.getLevel().registryAccess(), upgrade(key), true);
        }

        void removeOne(ResourceKey<Upgrade> key) {
            tile().getComponent().removeUpgrade(helper.getLevel().registryAccess(), upgrade(key), false);
        }
    }

    private static final class Scene extends Machine {
        private final String name;
        private final List<String> lines = new ArrayList<>();
        private final List<String> marks = new ArrayList<>();
        private int tick;
        private int refillTicks;
        private BooleanSupplier recheck;
        private int recheckOffset = -1;
        private WarningTracker warningTracker;
        private BooleanSupplier energyWarning;

        Scene(String name, Kind kind) {
            super(kind);
            this.name = name;
        }

        void begin(MekGameTestHelper helper, int speed, int energy) {
            place(helper, speed, energy);
        }

        /// Uma marca: o que o cenário fez depois daquele tick. As partes separadas por vírgula acontecem juntas.
        void apply(String mark) {
            marks.add(mark);
            for (String action : mark.split(", ")) {
                String[] w = action.split(" ");
                switch (w[0]) {
                    case "raw_iron" -> setInput(item("minecraft:raw_iron"), Integer.parseInt(w[1]));
                    case "clay" -> setInput(item("minecraft:clay"), Integer.parseInt(w[1]));
                    case "item" -> setInput(item("minecraft:" + w[1]), Integer.parseInt(w[2]));
                    case "tank" -> setTank(w[1], Long.parseLong(w[2]));
                    case "energy" -> {
                        if (w[1].equals("topped")) {
                            refillTicks = Integer.parseInt(w[4]);
                        } else {
                            energy().setEnergy(Long.parseLong(w[1]), null);
                        }
                    }
                    case "speed" -> setUpgrades(UpgradeIds.SPEED, Integer.parseInt(w[1]));
                    case "remove" -> {
                        if (w[1].equals("input")) {
                            setInput(item("minecraft:air"), 0);
                        } else if (w.length > 2) {
                            removeAll(UpgradeIds.SPEED);
                        } else {
                            removeOne(UpgradeIds.SPEED);
                        }
                    }
                    case "player" -> {
                        if (w[1].equals("opens")) {
                            tile().open(player);
                        } else {
                            tile().close(player);
                        }
                    }
                    default -> throw new IllegalStateException(action);
                }
            }
        }

        private BooleanSupplier recheck() {
            if (recheck == null) {
                try {
                    Field field = TileEntityRecipeMachine.class.getDeclaredField("recheckAllRecipeErrors");
                    field.setAccessible(true);
                    recheck = (BooleanSupplier) field.get(tile());
                    for (Field captured : recheck.getClass().getDeclaredFields()) {
                        if (captured.getType() == int.class) {
                            captured.setAccessible(true);
                            recheckOffset = captured.getInt(recheck);
                        }
                    }
                } catch (ReflectiveOperationException | RuntimeException e) {
                    throw new IllegalStateException(e);
                }
            }
            return recheck;
        }

        /// Os pares (tipo, verificação) que a tela da máquina registra (`GuiElectricMachine`, `GuiMetallurgicInfuser`) e os três de `GuiMekanismTile.addGenericTabs`.
        private WarningTracker warningTracker() {
            if (warningTracker == null) {
                TileEntityRecipeMachine<?, ?> tile = tile();
                WarningTracker tracker = new WarningTracker();
                if (kind.chemical) {
                    //GuiMetallurgicInfuser: o aviso da barra de químico é o do secundário, o do slot de entrada o do primário
                    tracker.trackWarning(WarningType.NOT_ENOUGH_ENERGY, tile.getWarningCheck(RecipeError.NOT_ENOUGH_ENERGY));
                    tracker.trackWarning(WarningType.INPUT_DOESNT_PRODUCE_OUTPUT, tile.getWarningCheck(RecipeError.INPUT_DOESNT_PRODUCE_OUTPUT));
                    tracker.trackWarning(WarningType.NO_MATCHING_RECIPE, tile.getWarningCheck(RecipeError.NOT_ENOUGH_SECONDARY_INPUT));
                    tracker.trackWarning(WarningType.NO_MATCHING_RECIPE, tile.getWarningCheck(RecipeError.NOT_ENOUGH_INPUT));
                    tracker.trackWarning(WarningType.NO_SPACE_IN_OUTPUT, tile.getWarningCheck(RecipeError.NOT_ENOUGH_OUTPUT_SPACE));
                } else {
                    tracker.trackWarning(WarningType.NO_MATCHING_RECIPE, tile.getWarningCheck(RecipeError.NOT_ENOUGH_INPUT));
                    tracker.trackWarning(WarningType.NO_SPACE_IN_OUTPUT, tile.getWarningCheck(RecipeError.NOT_ENOUGH_OUTPUT_SPACE));
                    tracker.trackWarning(WarningType.NOT_ENOUGH_ENERGY, tile.getWarningCheck(RecipeError.NOT_ENOUGH_ENERGY));
                    tracker.trackWarning(WarningType.INPUT_DOESNT_PRODUCE_OUTPUT, tile.getWarningCheck(RecipeError.INPUT_DOESNT_PRODUCE_OUTPUT));
                }
                tracker.trackWarning(WarningType.REDSTONE_SIGNAL_ABSENT, () -> tile.getControlType() == RedstoneControl.HIGH && !tile.isRedstoneActivated());
                tracker.trackWarning(WarningType.REDSTONE_SIGNAL_PRESENT, () -> tile.getControlType() == RedstoneControl.LOW && !tile.isRedstoneActivated());
                tracker.trackWarning(WarningType.REDSTONE_PULSE_REQUIRED, () -> tile.getControlType() == RedstoneControl.PULSE && !tile.isRedstoneActivated());
                warningTracker = tracker;
                energyWarning = tile.getWarningCheck(RecipeError.NOT_ENOUGH_ENERGY);
            }
            return warningTracker;
        }

        @SuppressWarnings("unchecked")
        private void appendWarnings(StringBuilder line) {
            try {
                WarningTracker tracker = warningTracker();
                Field field = WarningTracker.class.getDeclaredField("warnings");
                field.setAccessible(true);
                java.util.Map<WarningType, List<BooleanSupplier>> tracked = (java.util.Map<WarningType, List<BooleanSupplier>>) field.get(tracker);
                Field langEntry = WarningType.class.getDeclaredField("langEntry");
                langEntry.setAccessible(true);
                var shown = tracker.getWarnings();
                StringBuilder active = new StringBuilder();
                StringBuilder visible = new StringBuilder();
                for (WarningType type : WarningType.values()) {
                    List<BooleanSupplier> suppliers = tracked.get(type);
                    if (suppliers != null && suppliers.stream().anyMatch(BooleanSupplier::getAsBoolean)) {
                        active.append(active.isEmpty() ? "" : ",").append('"').append(type.name().toLowerCase()).append('"');
                    }
                    if (shown.contains(((mekanism.api.text.ILangEntry) langEntry.get(type)).translate())) {
                        visible.append(visible.isEmpty() ? "" : ",").append('"').append(type.name().toLowerCase()).append('"');
                    }
                }
                //GuiBar.drawBackground: o nível da barra de energia e o aviso extra por cima do conteúdo
                double level = ContainerType.ENERGY.divideToLevel(energy());
                boolean overlay = energyWarning.getAsBoolean() && level > 0 && level >= OVERLAY_LEVEL;
                line.append(",\"mode\":\"").append(tile().getControlType().name().toLowerCase()).append("\",\"warnings\":[").append(active)
                      .append("],\"warnings_shown\":[").append(visible).append("],\"bar_overlay\":").append(overlay);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private static void slot(StringBuilder line, ItemStack stack) {
            line.append('"').append(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath()).append("\",").append(stack.getCount());
        }

        void record() {
            if (helper == null || tile() == null) {
                return;
            }
            TileEntityProgressMachine<?> machine = machine();
            MachineEnergyContainer<?> energy = energy();
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++).append(",\"marks\":[");
            for (int i = 0; i < marks.size(); i++) {
                line.append(i == 0 ? "" : ",").append('"').append(marks.get(i)).append('"');
            }
            marks.clear();
            line.append("],\"progress\":").append(machine.getRecipeProgress())
                  .append(",\"required\":").append(machine.getTicksRequired())
                  .append(",\"energy\":").append(energy.getAmountAsLong())
                  .append(",\"capacity\":").append(energy.getCapacityAsLong())
                  .append(",\"per_tick\":").append(energy.getEnergyPerTick())
                  .append(",\"active\":").append(machine.getActive())
                  .append(",\"block_active\":").append(Attribute.isActive(machine.getBlockState()))
                  .append(",\"powered\":").append(machine.isPowered())
                  .append(",\"comparator\":").append(machine.getCurrentRedstoneLevel())
                  .append(",\"input\":[");
            slot(line, slot(kind.input).asItemStack());
            line.append("],\"output\":[");
            slot(line, slot(kind.output).asItemStack());
            line.append("],\"errors\":[");
            boolean first = true;
            for (int i = 0; i < ERRORS.size(); i++) {
                if (machine.getWarningCheck(ERRORS.get(i)).getAsBoolean()) {
                    line.append(first ? "" : ",").append('"').append(ERROR_NAMES.get(i)).append('"');
                    first = false;
                }
            }
            line.append(']');
            if (kind.chemical) {
                TileEntityMetallurgicInfuser infuser = (TileEntityMetallurgicInfuser) machine;
                line.append(",\"tank\":").append(infuser.infusionTank.amountAsLong()).append(",\"infusion\":[");
                slot(line, slot(0).asItemStack());
                line.append(']');
            }
            line.append(",\"game_time\":").append(helper.getLevel().getGameTime())
                  .append(",\"players\":").append(machine.playersUsing.size())
                  .append(",\"recheck\":").append(recheck().getAsBoolean())
                  .append(",\"recheck_offset\":").append(recheckOffset);
            appendWarnings(line);
            lines.add(line.append('}').toString());
            if (refillTicks > 0) {
                energy.setEnergy(refillTicks * (long) energy.getEnergyPerTick(), null);
            }
        }

        void write() {
            helper = null;
            WarningsTraceTest.write(name, lines);
        }
    }

    // ---------------------------------------------------------------------------------------------------------------- a varredura

    /// Um caso da varredura: uma receita com a entrada (e o tanque) numa quantidade, e a velocidade.
    private record Case(String recipe, String item, int need, int count, String chemical, long needAmount, long amount, int speed) {
    }

    /// Uma receita por vez, em ciclos de 5 ticks: no fim do tick 0 o caso é posto (energia cheia), os ticks 1 a 3 são gravados (o 3 limpa a máquina) e
    /// o tick 4 roda vazio, para o próximo caso começar de uma máquina em repouso. Cada linha: os erros que apareceram nos três ticks, o progresso
    /// e se a máquina esteve ativa em cada um.
    private static final class Sweep extends Machine {
        private static final int PERIOD = 5;
        private final List<Case> cases = new ArrayList<>();
        private final List<String> lines = new ArrayList<>();
        private int tick;
        private int index;
        private boolean done;
        private final List<Integer> progress = new ArrayList<>();
        private final List<Boolean> active = new ArrayList<>();
        private final Set<String> errors = new LinkedHashSet<>();
        private int stacked;

        Sweep(Kind kind) {
            super(kind);
        }

        boolean finished() {
            return done;
        }

        void begin(MekGameTestHelper helper) {
            place(helper, 0, 0);
            var level = helper.getLevel();
            Collection<? extends RecipeHolder<?>> holders = switch (kind) {
                case CHAMBER -> MekanismRecipeType.ENRICHING.getRecipes(level);
                case CRUSHER -> MekanismRecipeType.CRUSHING.getRecipes(level);
                case SMELTER -> MekanismRecipeType.SMELTING.getRecipes(level);
                case INFUSER -> MekanismRecipeType.METALLURGIC_INFUSING.getRecipes(level);
            };
            List<Case> fast = new ArrayList<>();
            for (RecipeHolder<?> holder : holders) {
                String id = holder.id().identifier().toString();
                var sized = kind.chemical
                      ? ((ItemStackChemicalToItemStackRecipe<?, ?>) holder.value()).getItemInput().ingredient()
                      : ((ItemStackToItemStackRecipe<?, ?>) holder.value()).getInput().ingredient();
                if (sized.ingredient().isCustom() || sized.ingredient().getValues().size() == 0) {
                    continue;
                }
                Item item = sized.ingredient().getValues().stream().findFirst().orElseThrow().value();
                String itemId = BuiltInRegistries.ITEM.getKey(item).toString();
                int need = sized.count();
                int stack = Math.max(1, Math.min(item.getDefaultMaxStackSize(), 64));
                Set<Integer> counts = new LinkedHashSet<>();
                counts.add(Math.min(1, stack));
                if (need > 1) {
                    counts.add(Math.min(need - 1, stack));
                }
                counts.add(Math.min(need, stack));
                counts.add(Math.min(need + 1, stack));
                if (!kind.chemical) {
                    for (int count : counts) {
                        cases.add(new Case(id, itemId, need, count, "", 0, 0, 0));
                    }
                    int big = Math.min(need * 3, stack);
                    fast.add(new Case(id, itemId, need, Math.min(need, stack), "", 0, 0, 8));
                    fast.add(new Case(id, itemId, need, big, "", 0, 0, 8));
                    continue;
                }
                var chemicalInput = ((ItemStackChemicalToItemStackRecipe<?, ?>) holder.value()).getChemicalInput();
                var chemicals = chemicalInput.ingredient().chemicals();
                if (chemicals.isEmpty()) {
                    continue;
                }
                String chemical = chemicals.get(0).getRegisteredName();
                long amount = chemicalInput.amount();
                Set<Long> amounts = new LinkedHashSet<>();
                amounts.add(amount - 1);
                amounts.add(amount);
                amounts.add(amount + 1);
                amounts.add(1000L);
                for (int count : counts) {
                    for (long tank : amounts) {
                        if (tank > 0) {
                            cases.add(new Case(id, itemId, need, count, chemical, amount, tank, 0));
                        }
                    }
                }
                fast.add(new Case(id, itemId, need, Math.min(need, stack), chemical, amount, amount, 8));
                fast.add(new Case(id, itemId, need, Math.min(need * 3, stack), chemical, amount, 1000, 8));
            }
            cases.addAll(fast);
        }

        void onTick() {
            if (helper == null || done || tile() == null) {
                return;
            }
            if (index >= cases.size()) {
                done = true;
                return;
            }
            Case current = cases.get(index);
            int step = tick++ % PERIOD;
            if (step == 0) {
                if (current.speed != tile().getComponent().getUpgrades(upgrade(UpgradeIds.SPEED))) {
                    setUpgrades(UpgradeIds.SPEED, current.speed);
                }
                setInput(item(current.item), current.count);
                if (kind.chemical) {
                    setTank(helper.getLevel().registryAccess().getOrThrow(ResourceKey.create(MekanismRegistries.Keys.CHEMICAL,
                          net.minecraft.resources.Identifier.parse(current.chemical))), current.amount);
                }
                energy().setEnergy(energy().getCapacityAsLong(), null);
                progress.clear();
                active.clear();
                errors.clear();
                stacked = slot(kind.input).asItemStack().getCount();
            } else if (step <= 3) {
                TileEntityProgressMachine<?> machine = machine();
                progress.add(machine.getRecipeProgress());
                active.add(machine.getActive());
                for (int i = 0; i < ERRORS.size(); i++) {
                    if (machine.getWarningCheck(ERRORS.get(i)).getAsBoolean()) {
                        errors.add(ERROR_NAMES.get(i));
                    }
                }
                energy().setEnergy(energy().getCapacityAsLong(), null);
                if (step == 3) {
                    lines.add("{\"recipe\":\"" + current.recipe + "\",\"item\":\"" + current.item + "\",\"need\":" + current.need + ",\"count\":" + current.count + ",\"stacked\":" + stacked
                          + ",\"chemical\":\"" + current.chemical + "\",\"need_amount\":" + current.needAmount + ",\"amount\":" + current.amount + ",\"speed\":" + current.speed
                          + ",\"errors\":" + quoted(errors) + ",\"progress\":" + progress + ",\"active\":" + active + ",\"output\":" + outputJson() + "}");
                    setInput(item("minecraft:air"), 0);
                    if (kind.chemical) {
                        ((TileEntityMetallurgicInfuser) tile()).infusionTank.setContents(ChemicalResource.EMPTY, 0, null);
                    }
                    slot(kind.output).setContents(ItemResource.EMPTY, 0, null);
                    index++;
                }
            }
        }

        private String outputJson() {
            ItemStack stack = slot(kind.output).asItemStack();
            return "[\"" + (stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath()) + "\"," + stack.getCount() + "]";
        }

        private static String quoted(Set<String> values) {
            StringBuilder out = new StringBuilder("[");
            for (String value : values) {
                out.append(out.length() == 1 ? "" : ",").append('"').append(value).append('"');
            }
            return out.append(']').toString();
        }

        void write() {
            WarningsTraceTest.write("warnings_sweep_" + kind.id, lines);
        }
    }
}
