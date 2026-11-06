package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.lang.reflect.Field;
import java.util.List;
import java.util.Map;
import java.util.function.BooleanSupplier;
import java.util.function.Supplier;
import mekanism.api.MekanismRegistries;
import mekanism.api.RelativeSide;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.chemical.IChemicalTank;
import mekanism.api.inventory.IInventorySlot;
import mekanism.api.recipes.cache.CachedRecipe.OperationTracker.RecipeError;
import mekanism.api.text.ILangEntry;
import mekanism.api.upgrade.IUpgradeHelper;
import mekanism.api.upgrade.Upgrade;
import mekanism.api.upgrade.UpgradeIds;
import mekanism.common.block.attribute.Attribute;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.inventory.warning.WarningTracker;
import mekanism.common.inventory.warning.WarningTracker.WarningType;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismItems;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.tile.interfaces.IRedstoneControl.RedstoneControl;
import mekanism.common.tile.machine.TileEntityElectrolyticSeparator;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import mekanism.common.tile.prefab.TileEntityElectricMachine;
import mekanism.common.tile.prefab.TileEntityProgressMachine;
import mekanism.common.tile.prefab.TileEntityRecipeMachine;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.world.Container;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o estado da Enrichment Chamber a cada tick, para os testes em Rust compararem com o `CachedRecipe` e a base de tile.
/// A câmara fica em (1, 0, 0) e a posição (0, 0, 0) recebe o bloco de redstone. Cada linha do `.jsonl` é um tick.
@ForEachTest(groups = "trace")
public class MachineTraceTest {

    private static final BlockPos CHAMBER = new BlockPos(1, 0, 0);
    private static final BlockPos CHEST = new BlockPos(2, 0, 0);
    private static final int INPUT = 0;
    private static final int OUTPUT = 1;
    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(2, 1, 1)
              .fill(1, 0, 0, 1, 0, 0, MekanismBlocks.ENRICHMENT_CHAMBER.defaultState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records a full operation, then running out of energy and getting it back.")
    public static void chamberEnergy(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_energy");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("insert clay 3, energy 1200");
                  trace.setSlot(INPUT, Items.CLAY, 3);
                  trace.setEnergy(1_200);
              })
              .thenExecuteAfter(260, () -> {
                  trace.mark("energy 500");
                  trace.setEnergy(500);
              })
              .thenExecuteAfter(120, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the input being removed in the middle of an operation and put back.")
    public static void chamberInputRemoved(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_input_removed");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("insert clay 1, energy 20000");
                  trace.setSlot(INPUT, Items.CLAY, 1);
                  trace.setEnergy(20_000);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("remove input");
                  trace.clearSlot(INPUT);
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("insert clay 1");
                  trace.setSlot(INPUT, Items.CLAY, 1);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("swap to stone");
                  trace.setSlot(INPUT, Items.STONE, 1);
              })
              .thenExecuteAfter(100, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records an output with no room for the recipe and another with a different item.")
    public static void chamberOutput(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_output");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("clay 2, output clay_ball 63, energy 20000");
                  trace.setSlot(INPUT, Items.CLAY, 2);
                  trace.setSlot(OUTPUT, Items.CLAY_BALL, 63);
                  trace.setEnergy(20_000);
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("output clay_ball 60");
                  trace.setSlot(OUTPUT, Items.CLAY_BALL, 60);
              })
              .thenExecuteAfter(210, () -> {
                  trace.mark("output stone 1");
                  trace.setSlot(OUTPUT, Items.STONE, 1);
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("clear output");
                  trace.clearSlot(OUTPUT);
              })
              .thenExecuteAfter(220, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the redstone control modes while the signal goes on and off.")
    public static void chamberRedstone(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_redstone");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("clay 64, energy 20000, mode high");
                  trace.setSlot(INPUT, Items.CLAY, 64);
                  trace.setEnergy(20_000);
                  trace.chamber().setControlType(RedstoneControl.HIGH);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("signal off");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("mode low");
                  trace.chamber().setControlType(RedstoneControl.LOW);
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal off, mode pulse");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
                  trace.chamber().setControlType(RedstoneControl.PULSE);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal off");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
              })
              .thenExecuteAfter(10, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the deactivation delay of the block state, with the machine stopping and restarting within it.")
    public static void chamberActiveDelay(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_active_delay");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("clay 64, energy 20000");
                  trace.setSlot(INPUT, Items.CLAY, 64);
                  trace.setEnergy(20_000);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("energy 0");
                  trace.setEnergy(0);
              })
              .thenExecuteAfter(70, () -> {
                  trace.mark("energy 20000");
                  trace.setEnergy(20_000);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("energy 0");
                  trace.setEnergy(0);
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("energy 20000");
                  trace.setEnergy(20_000);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("energy 0");
                  trace.setEnergy(0);
              })
              .thenExecuteAfter(80, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records ticks required, energy per tick and capacity as speed and energy upgrades go in.")
    public static void chamberUpgrades(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_upgrades");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("clay 64");
                  trace.setSlot(INPUT, Items.CLAY, 64);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("speed 1");
                  trace.addUpgrades(UpgradeIds.SPEED, 1);
                  trace.setEnergy(trace.energy().getCapacityAsLong());
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("speed 8");
                  trace.addUpgrades(UpgradeIds.SPEED, 7);
                  trace.setEnergy(trace.energy().getCapacityAsLong());
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("energy 8");
                  trace.addUpgrades(UpgradeIds.ENERGY, 8);
                  trace.setEnergy(trace.energy().getCapacityAsLong());
              })
              .thenExecuteAfter(40, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records a recipe that needs three items running with fewer than that in the input.")
    public static void chamberInputShort(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_input_short");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("hdpe_pellet 5, energy 20000");
                  trace.setSlot(INPUT, MekanismItems.HDPE_PELLET.asItem(), 5);
                  trace.setEnergy(20_000);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("hdpe_pellet 2");
                  trace.setSlot(INPUT, MekanismItems.HDPE_PELLET.asItem(), 2);
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("hdpe_pellet 3");
                  trace.setSlot(INPUT, MekanismItems.HDPE_PELLET.asItem(), 3);
              })
              .thenExecuteAfter(220, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records a paused recipe ignoring changes that do not notify it: redstone mode and upgrades.")
    public static void chamberPaused(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_paused");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("clay 64, energy 3");
                  trace.setSlot(INPUT, Items.CLAY, 64);
                  trace.setEnergy(3);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("mode high");
                  trace.chamber().setControlType(RedstoneControl.HIGH);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("energy 20000");
                  trace.setEnergy(20_000);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("energy 3");
                  trace.setEnergy(3);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("upgrade energy 8");
                  trace.addUpgrades(UpgradeIds.ENERGY, 8);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("energy 4");
                  trace.setEnergy(4);
              })
              .thenExecuteAfter(20, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the output ejecting into a chest, one transaction across two blocks, with room for all, part and none.")
    public static void chamberEject(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 1, 1)
              .fill(1, 0, 0, 1, 0, 0, MekanismBlocks.ENRICHMENT_CHAMBER.defaultState())
              .fill(2, 0, 0, 2, 0, 0, Blocks.CHEST.defaultBlockState()));
        Trace trace = new Trace("chamber_eject");
        trace.recordChest = true;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("eject on, output clay_ball 40");
                  trace.ejectEast();
                  trace.setSlot(OUTPUT, Items.CLAY_BALL, 40);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("chest room 2, output clay_ball 5");
                  trace.fillChest(Items.STONE, 64);
                  trace.chest().setItem(26, new ItemStack(Items.CLAY_BALL, 62));
                  trace.setSlot(OUTPUT, Items.CLAY_BALL, 5);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("chest room 0, output clay_ball 5");
                  trace.fillChest(Items.STONE, 64);
                  trace.setSlot(OUTPUT, Items.CLAY_BALL, 5);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("chest clear");
                  trace.chest().clearContent();
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("clay 1, energy 20000");
                  trace.setSlot(INPUT, Items.CLAY, 1);
                  trace.setEnergy(20_000);
              })
              .thenExecuteAfter(230, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the output ejecting into a furnace, whose side only takes fuel.")
    public static void chamberEjectFurnace(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 1, 1)
              .fill(1, 0, 0, 1, 0, 0, MekanismBlocks.ENRICHMENT_CHAMBER.defaultState())
              .fill(2, 0, 0, 2, 0, 0, Blocks.FURNACE.defaultBlockState()));
        Trace trace = new Trace("chamber_eject_furnace");
        trace.recordChest = true;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("eject on, output clay_ball 5");
                  trace.ejectEast();
                  trace.setSlot(OUTPUT, Items.CLAY_BALL, 5);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("output coal 5");
                  trace.setSlot(OUTPUT, Items.COAL, 5);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("output charcoal 5");
                  trace.setSlot(OUTPUT, Items.CHARCOAL, 5);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("output coal 64");
                  trace.setSlot(OUTPUT, Items.COAL, 64);
              })
              .thenExecuteAfter(30, trace::write)
              .thenSucceed()
        );
    }

    private static void template(DynamicTest test, Supplier<BlockState> state) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(2, 1, 1).fill(1, 0, 0, 1, 0, 0, state.get()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the Crusher, an electric machine with another usage and its own recipes.")
    public static void crusherRun(final DynamicTest test) {
        template(test, MekanismBlocks.CRUSHER::defaultState);
        Trace trace = new Trace("crusher_run");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("item cobblestone 2, energy 2000");
                  trace.setSlot(INPUT, Items.COBBLESTONE, 2);
                  trace.setEnergy(2_000);
              })
              .thenExecuteAfter(270, () -> {
                  trace.mark("energy 1000");
                  trace.setEnergy(1_000);
              })
              .thenExecuteAfter(150, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the Energized Smelter on the game's cooking recipes.")
    public static void smelterRun(final DynamicTest test) {
        template(test, MekanismBlocks.ENERGIZED_SMELTER::defaultState);
        Trace trace = new Trace("smelter_run");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("item raw_iron 2, energy 1200");
                  trace.setSlot(INPUT, Items.RAW_IRON, 2);
                  trace.setEnergy(1_200);
              })
              .thenExecuteAfter(260, () -> {
                  trace.mark("energy 500");
                  trace.setEnergy(500);
              })
              .thenExecuteAfter(120, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the Metallurgic Infuser: two inputs, a tank that runs short, an item with no recipe and items converted into the tank.")
    public static void infuserRun(final DynamicTest test) {
        template(test, MekanismBlocks.METALLURGIC_INFUSER::defaultState);
        Trace trace = new Trace("infuser_run");
        trace.inputSlot = 1;
        trace.outputSlot = 2;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("item copper_ingot 2, tank redstone 15, energy 3000");
                  trace.setSlot(1, Items.COPPER_INGOT, 2);
                  trace.setTank(15);
                  trace.setEnergy(3_000);
              })
              .thenExecuteAfter(230, () -> {
                  trace.mark("tank redstone 30");
                  trace.setTank(30);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("tank redstone 0");
                  trace.setTank(0);
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("tank redstone 30");
                  trace.setTank(30);
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("item iron_ingot 1");
                  trace.setSlot(1, Items.IRON_INGOT, 1);
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("item copper_ingot 1, tank redstone 0");
                  trace.setSlot(1, Items.COPPER_INGOT, 1);
                  trace.setTank(0);
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("infusion redstone 3");
                  trace.setSlot(0, Items.REDSTONE, 3);
              })
              .thenExecuteAfter(240, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the upgrade slots: installing over 20 ticks, past the maximum, removing one and removing all.")
    public static void chamberUpgradeSlot(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_upgrade_slot");
        trace.recordUpgrades = true;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("upgrade slot speed 3");
                  trace.setUpgradeSlot(UpgradeIds.SPEED, 3);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("upgrade slot speed 10");
                  trace.setUpgradeSlot(UpgradeIds.SPEED, 10);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("upgrade slot energy 2");
                  trace.setUpgradeSlot(UpgradeIds.ENERGY, 2);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("upgrade slot clear");
                  trace.clearUpgradeSlot();
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("upgrade slot energy 1");
                  trace.setUpgradeSlot(UpgradeIds.ENERGY, 1);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("remove speed");
                  trace.removeUpgrade(UpgradeIds.SPEED, false);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("remove speed all");
                  trace.removeUpgrade(UpgradeIds.SPEED, true);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("remove energy all");
                  trace.removeUpgrade(UpgradeIds.ENERGY, true);
              })
              .thenExecuteAfter(10, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records a chamber placed from its item, as a player does: the upgrades recalculate, unlike with setBlock.")
    public static void chamberPlacedByPlayer(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(2, 2, 1)
              .fill(1, 0, 0, 1, 0, 0, Blocks.STONE.defaultBlockState()));
        Trace trace = new Trace("chamber_placed");
        trace.pos = new BlockPos(1, 1, 0);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Player player = helper.makeMockPlayerLookingAt(1, 0, 0, Direction.UP);
                  helper.useOn(new BlockPos(1, 0, 0), new ItemStack(MekanismBlocks.ENRICHMENT_CHAMBER.asItem()), player, Direction.UP);
              })
              .thenExecuteAfter(2, () -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("energy 500");
                  trace.setEnergy(500);
              })
              .thenExecuteAfter(5, trace::write)
              .thenSucceed()
        );
    }

    private static void recheckSetup(Trace trace) {
        trace.mark("clay 64, output clay_ball 64, energy 0");
        trace.setSlot(INPUT, Items.CLAY, 64);
        trace.setSlot(OUTPUT, Items.CLAY_BALL, 64);
        trace.setEnergy(0);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the full-recheck flag with a player in the GUI: it comes back every 100 ticks, at a random offset.")
    public static void chamberRecheckLooking(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_recheck_looking");
        trace.recordRecheck = true;
        trace.recordWarnings = true;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  recheckSetup(trace);
                  trace.mark("player opens");
                  trace.openPlayer();
              })
              .thenExecuteAfter(250, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the same chamber with nobody in the GUI: the full recheck never runs.")
    public static void chamberRecheckAlone(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_recheck_alone");
        trace.recordRecheck = true;
        trace.recordWarnings = true;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> recheckSetup(trace))
              .thenExecuteAfter(250, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the full-recheck flag while a player opens, closes and opens the GUI again.")
    public static void chamberRecheckOpenClose(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_recheck_open_close");
        trace.recordRecheck = true;
        trace.recordWarnings = true;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  recheckSetup(trace);
                  trace.mark("player opens");
                  trace.openPlayer();
              })
              .thenExecuteAfter(130, () -> {
                  trace.mark("player closes");
                  trace.closePlayer();
              })
              .thenExecuteAfter(130, () -> {
                  trace.mark("player opens");
                  trace.openPlayer();
              })
              .thenExecuteAfter(130, trace::write)
              .thenSucceed()
        );
    }

    private static void recheckChangesErrors(DynamicTest test, String name, boolean looking) {
        template(test);
        Trace trace = new Trace(name);
        trace.recordRecheck = true;
        trace.recordWarnings = true;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("clay 64, energy 0");
                  trace.setSlot(INPUT, Items.CLAY, 64);
                  trace.setEnergy(0);
                  if (looking) {
                      trace.mark("player opens");
                      trace.openPlayer();
                  }
              })
              //Only the energy error is known by now. The recipe is paused, and the output slot below wakes it up without a full check.
              .thenExecuteAfter(20, () -> {
                  trace.mark("output clay_ball 64");
                  trace.setSlot(OUTPUT, Items.CLAY_BALL, 64);
              })
              .thenExecuteAfter(230, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records a woken recipe that knows only the energy error: the full recheck finds the output space error with a player in the GUI.")
    public static void chamberRecheckChangesErrors(final DynamicTest test) {
        recheckChangesErrors(test, "chamber_recheck_changes_errors", true);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the same woken recipe with nobody in the GUI: the output space error is never found.")
    public static void chamberRecheckChangesErrorsAlone(final DynamicTest test) {
        recheckChangesErrors(test, "chamber_recheck_changes_errors_alone", false);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the chamber with speed 8 and energy topped up to a few ticks: it has no reduced rate to report.")
    public static void chamberReducedRate(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_reduced_rate");
        trace.recordRecheck = true;
        trace.recordWarnings = true;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("clay 64, speed 8, player opens");
                  trace.setSlot(INPUT, Items.CLAY, 64);
                  trace.addUpgrades(UpgradeIds.SPEED, 8);
                  trace.openPlayer();
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("energy topped up to 3 ticks every tick");
                  trace.refillTicks = 3;
              })
              .thenExecuteAfter(40, () -> {
                  trace.mark("energy topped up to a third of a tick every tick");
                  trace.refillTicks = 0;
                  trace.refillFraction = 3;
              })
              .thenExecuteAfter(40, () -> {
                  trace.mark("no more top ups, energy 0");
                  trace.refillFraction = 0;
                  trace.setEnergy(0);
              })
              .thenExecuteAfter(30, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the Electrolytic Separator at speed 8, the only machine that reports not enough energy for the maximum rate.")
    public static void separatorReducedRate(final DynamicTest test) {
        template(test, MekanismBlocks.ELECTROLYTIC_SEPARATOR::defaultState);
        Trace trace = new Trace("separator_reduced_rate");
        trace.separator = true;
        trace.recordRecheck = true;
        trace.recordWarnings = true;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("speed 8, water 20000, player opens");
                  trace.addUpgrades(UpgradeIds.SPEED, 8);
                  trace.setWater(20_000);
                  trace.openPlayer();
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("energy topped up to 100 operations every tick");
                  trace.refillTicks = 100;
              })
              .thenExecuteAfter(8, () -> {
                  trace.mark("energy topped up to 256 operations every tick, tanks emptied");
                  trace.setTanks(0);
                  trace.refillTicks = 256;
              })
              .thenExecuteAfter(4, () -> {
                  trace.mark("energy topped up to 30 operations every tick, tanks emptied");
                  trace.setTanks(0);
                  trace.refillTicks = 30;
              })
              .thenExecuteAfter(8, () -> {
                  trace.mark("no more top ups, energy 0, tanks emptied");
                  trace.setTanks(0);
                  trace.refillTicks = 0;
                  trace.setEnergy(0);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("mode high, no signal");
                  trace.separator().setControlType(RedstoneControl.HIGH);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("energy topped up to 10 operations");
                  trace.setEnergy(10L * trace.energy().getEnergyPerTick());
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(10, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the warnings of the redstone modes with a player in the GUI, with and without the energy error behind them.")
    public static void chamberWarningsRedstone(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_warnings_redstone");
        trace.recordRecheck = true;
        trace.recordWarnings = true;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("clay 64, energy 20000, player opens");
                  trace.setSlot(INPUT, Items.CLAY, 64);
                  trace.setEnergy(20_000);
                  trace.openPlayer();
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("mode high, no signal");
                  trace.chamber().setControlType(RedstoneControl.HIGH);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal off");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("mode low, no signal");
                  trace.chamber().setControlType(RedstoneControl.LOW);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("mode pulse, signal on");
                  trace.chamber().setControlType(RedstoneControl.PULSE);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal off");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal off, mode disabled, energy 0");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
                  trace.chamber().setControlType(RedstoneControl.DISABLED);
                  trace.setEnergy(0);
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("mode high, no signal, with the energy error behind it");
                  trace.chamber().setControlType(RedstoneControl.HIGH);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("mode low, signal on, with the energy error behind it");
                  trace.chamber().setControlType(RedstoneControl.LOW);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("signal off");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
              })
              .thenExecuteAfter(10, trace::write)
              .thenSucceed()
        );
    }

    private static class Trace {

        private static final List<RecipeError> ERRORS = List.of(RecipeError.NOT_ENOUGH_ENERGY, RecipeError.NOT_ENOUGH_INPUT,
              RecipeError.NOT_ENOUGH_OUTPUT_SPACE, RecipeError.INPUT_DOESNT_PRODUCE_OUTPUT, RecipeError.NOT_ENOUGH_SECONDARY_INPUT);
        private static final List<String> ERROR_NAMES = List.of("energy", "input", "output_space", "wrong_output", "secondary");

        private final String name;
        private final List<String> lines = new ArrayList<>();
        private final List<String> marks = new ArrayList<>();
        private MekGameTestHelper helper;
        private int tick;
        boolean recordChest;
        boolean recordUpgrades;
        BlockPos pos = CHAMBER;
        int inputSlot = INPUT;
        int outputSlot = OUTPUT;
        boolean separator;
        boolean recordRecheck;
        boolean recordWarnings;
        /// A cada tick, depois de gravar, devolve ao container energia para esta quantidade de ticks da máquina (ou de operações, no separador).
        int refillTicks;
        /// Se maior que zero, devolve 1/refillFraction do consumo de um tick.
        int refillFraction;
        private Player player;
        private WarningTracker warningTracker;
        private BooleanSupplier recheck;
        private int recheckOffset = -1;

        Trace(String name) {
            this.name = name;
        }

        void start(MekGameTestHelper helper) {
            this.helper = helper;
        }

        void mark(String mark) {
            marks.add(mark);
        }

        TileEntityProgressMachine<?> machine() {
            BlockEntity tile = helper.getLevel().getBlockEntity(helper.absolutePos(pos));
            return tile instanceof TileEntityProgressMachine<?> machine ? machine : null;
        }

        TileEntityElectricMachine<?, ?> chamber() {
            return machine() instanceof TileEntityElectricMachine<?, ?> chamber ? chamber : null;
        }

        TileEntityRecipeMachine<?, ?> tile() {
            BlockEntity tile = helper.getLevel().getBlockEntity(helper.absolutePos(pos));
            return tile instanceof TileEntityRecipeMachine<?, ?> machine ? machine : null;
        }

        TileEntityElectrolyticSeparator separator() {
            return (TileEntityElectrolyticSeparator) tile();
        }

        MachineEnergyContainer<?> energy() {
            return (MachineEnergyContainer<?>) tile().getEnergyContainer();
        }

        void setWater(long amount) {
            separator().fluidTank.setContents(FluidResource.of(Fluids.WATER), amount, null);
        }

        void setTanks(long ignored) {
            TileEntityElectrolyticSeparator separator = separator();
            separator.leftTank.setContents(ChemicalResource.EMPTY, 0, null);
            separator.rightTank.setContents(ChemicalResource.EMPTY, 0, null);
        }

        /// Abre o tile com um jogador falso olhando para ele, como a tela faz: entra em `playersUsing`.
        void openPlayer() {
            if (player == null) {
                player = helper.makeMockPlayerLookingAt(pos, Direction.NORTH);
            }
            tile().open(player);
        }

        void closePlayer() {
            tile().close(player);
        }

        void setTank(long amount) {
            IChemicalTank tank = ((TileEntityMetallurgicInfuser) machine()).infusionTank;
            Holder<Chemical> redstone = helper.getLevel().registryAccess().getOrThrow(ChemicalIds.REDSTONE);
            tank.setContents(amount == 0 ? ChemicalResource.EMPTY : ChemicalResource.of(redstone), amount, null);
        }

        Container chest() {
            return (Container) helper.getLevel().getBlockEntity(helper.absolutePos(CHEST));
        }

        void fillChest(Item item, int count) {
            for (int i = 0; i < chest().getContainerSize(); i++) {
                chest().setItem(i, new ItemStack(item, count));
            }
        }

        /// Liga o ejetor de itens e põe a saída no lado do baú.
        void ejectEast() {
            ConfigInfo info = chamber().getConfig().getConfig(TransmissionType.ITEM);
            for (RelativeSide side : RelativeSide.values()) {
                if (side.getDirection(chamber().getDirection()) == Direction.EAST) {
                    info.setDataType(DataType.OUTPUT, side);
                    chamber().getConfig().sideChanged(TransmissionType.ITEM, side);
                }
            }
            info.setEjecting(true);
        }

        Holder<Upgrade> upgrade(net.minecraft.resources.ResourceKey<Upgrade> key) {
            return helper.getLevel().registryAccess().lookupOrThrow(MekanismRegistries.Keys.UPGRADES).getOrThrow(key);
        }

        void setUpgradeSlot(net.minecraft.resources.ResourceKey<Upgrade> key, int count) {
            machine().getComponent().getUpgradeSlot().setContents(IUpgradeHelper.INSTANCE.asResource(upgrade(key)), count, null);
        }

        void clearUpgradeSlot() {
            machine().getComponent().getUpgradeSlot().setContents(ItemResource.EMPTY, 0, null);
        }

        void removeUpgrade(net.minecraft.resources.ResourceKey<Upgrade> key, boolean all) {
            machine().getComponent().removeUpgrade(helper.getLevel().registryAccess(), upgrade(key), all);
        }

        private void upgradeSlot(StringBuilder line, IInventorySlot slot) {
            ItemStack stack = slot.asItemStack();
            Holder<Upgrade> type = stack.isEmpty() ? null : stack.get(IUpgradeHelper.INSTANCE.dataComponent());
            line.append('"').append(type == null ? "" : type.getRegisteredName().substring("mekanism:".length())).append("\",").append(stack.getCount());
        }

        void setSlot(int slot, Item item, int count) {
            machine().getInventorySlots().get(slot).setContents(ItemResource.of(item), count, null);
        }

        void clearSlot(int slot) {
            machine().getInventorySlots().get(slot).setContents(ItemResource.EMPTY, 0, null);
        }

        void setEnergy(long energy) {
            energy().setEnergy(energy, null);
        }

        void addUpgrades(net.minecraft.resources.ResourceKey<Upgrade> key, int amount) {
            Holder<Upgrade> upgrade = helper.getLevel().registryAccess().lookupOrThrow(MekanismRegistries.Keys.UPGRADES).getOrThrow(key);
            tile().getComponent().addUpgrades(helper.getLevel().registryAccess(), upgrade, amount);
        }

        private static void slot(StringBuilder line, IInventorySlot slot) {
            ItemStack stack = slot.asItemStack();
            line.append('"').append(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath())
                  .append("\",").append(stack.getCount());
        }

        /// Lê por reflexão o `recheckAllRecipeErrors` do tile (protected final) e o deslocamento sorteado que a lambda captura.
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

        /// Monta o `WarningTracker` com os mesmos pares (tipo, verificação) que a tela registra, e lê por reflexão os tipos ativos.
        private WarningTracker warningTracker() {
            if (warningTracker == null) {
                TileEntityRecipeMachine<?, ?> tile = tile();
                WarningTracker tracker = new WarningTracker();
                if (tile instanceof TileEntityElectrolyticSeparator) {
                    //GuiElectrolyticSeparator
                    tracker.trackWarning(WarningType.NO_MATCHING_RECIPE, tile.getWarningCheck(RecipeError.NOT_ENOUGH_INPUT));
                    tracker.trackWarning(WarningType.NO_SPACE_IN_OUTPUT, tile.getWarningCheck(TileEntityElectrolyticSeparator.NOT_ENOUGH_SPACE_LEFT_OUTPUT_ERROR));
                    tracker.trackWarning(WarningType.NO_SPACE_IN_OUTPUT, tile.getWarningCheck(TileEntityElectrolyticSeparator.NOT_ENOUGH_SPACE_RIGHT_OUTPUT_ERROR));
                    tracker.trackWarning(WarningType.NOT_ENOUGH_ENERGY, tile.getWarningCheck(RecipeError.NOT_ENOUGH_ENERGY));
                    tracker.trackWarning(WarningType.NOT_ENOUGH_ENERGY_REDUCED_RATE, tile.getWarningCheck(RecipeError.NOT_ENOUGH_ENERGY_REDUCED_RATE));
                    tracker.trackWarning(WarningType.INPUT_DOESNT_PRODUCE_OUTPUT, tile.getWarningCheck(RecipeError.INPUT_DOESNT_PRODUCE_OUTPUT));
                } else {
                    //TileEntityElectricMachine (slots) and GuiElectricMachine
                    tracker.trackWarning(WarningType.NO_MATCHING_RECIPE, tile.getWarningCheck(RecipeError.NOT_ENOUGH_INPUT));
                    tracker.trackWarning(WarningType.NO_SPACE_IN_OUTPUT, tile.getWarningCheck(RecipeError.NOT_ENOUGH_OUTPUT_SPACE));
                    tracker.trackWarning(WarningType.NOT_ENOUGH_ENERGY, tile.getWarningCheck(RecipeError.NOT_ENOUGH_ENERGY));
                    tracker.trackWarning(WarningType.INPUT_DOESNT_PRODUCE_OUTPUT, tile.getWarningCheck(RecipeError.INPUT_DOESNT_PRODUCE_OUTPUT));
                }
                //GuiMekanismTile.addGenericTabs
                tracker.trackWarning(WarningType.REDSTONE_SIGNAL_ABSENT, () -> tile.getControlType() == RedstoneControl.HIGH && !tile.isRedstoneActivated());
                tracker.trackWarning(WarningType.REDSTONE_SIGNAL_PRESENT, () -> tile.getControlType() == RedstoneControl.LOW && !tile.isRedstoneActivated());
                tracker.trackWarning(WarningType.REDSTONE_PULSE_REQUIRED, () -> tile.getControlType() == RedstoneControl.PULSE && !tile.isRedstoneActivated());
                warningTracker = tracker;
            }
            return warningTracker;
        }

        @SuppressWarnings("unchecked")
        private void appendWarnings(StringBuilder line) {
            try {
                WarningTracker tracker = warningTracker();
                Field field = WarningTracker.class.getDeclaredField("warnings");
                field.setAccessible(true);
                Map<WarningType, List<BooleanSupplier>> tracked = (Map<WarningType, List<BooleanSupplier>>) field.get(tracker);
                Field langEntry = WarningType.class.getDeclaredField("langEntry");
                langEntry.setAccessible(true);
                List<Component> shown = tracker.getWarnings();
                StringBuilder active = new StringBuilder();
                StringBuilder visible = new StringBuilder();
                for (WarningType type : WarningType.values()) {
                    List<BooleanSupplier> suppliers = tracked.get(type);
                    if (suppliers != null && suppliers.stream().anyMatch(BooleanSupplier::getAsBoolean)) {
                        active.append(active.isEmpty() ? "" : ",").append('"').append(type.name().toLowerCase()).append('"');
                    }
                    if (shown.contains(((ILangEntry) langEntry.get(type)).translate())) {
                        visible.append(visible.isEmpty() ? "" : ",").append('"').append(type.name().toLowerCase()).append('"');
                    }
                }
                line.append(",\"mode\":\"").append(tile().getControlType().name().toLowerCase()).append("\",\"warnings\":[").append(active)
                      .append("],\"warnings_shown\":[").append(visible).append(']');
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private void appendExtras(StringBuilder line) {
            if (recordRecheck) {
                boolean flag = recheck().getAsBoolean();
                line.append(",\"game_time\":").append(helper.getLevel().getGameTime())
                      .append(",\"players\":").append(tile().playersUsing.size())
                      .append(",\"recheck\":").append(flag)
                      .append(",\"recheck_offset\":").append(recheckOffset);
            }
            if (recordWarnings) {
                appendWarnings(line);
            }
        }

        /// Devolve energia depois de gravar o tick, para a máquina ver o mesmo valor no tick seguinte.
        private void refill() {
            if (refillTicks > 0) {
                setEnergy(refillTicks * (long) energy().getEnergyPerTick());
            } else if (refillFraction > 0) {
                setEnergy(energy().getEnergyPerTick() / refillFraction);
            }
        }

        private void recordSeparator() {
            TileEntityElectrolyticSeparator tile = separator();
            MachineEnergyContainer<?> energy = energy();
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++).append(",\"marks\":[");
            for (int i = 0; i < marks.size(); i++) {
                line.append(i == 0 ? "" : ",").append('"').append(marks.get(i)).append('"');
            }
            marks.clear();
            line.append("],\"energy\":").append(energy.getAmountAsLong())
                  .append(",\"capacity\":").append(energy.getCapacityAsLong())
                  .append(",\"per_tick\":").append(energy.getEnergyPerTick())
                  .append(",\"active\":").append(tile.getActive())
                  .append(",\"block_active\":").append(Attribute.isActive(tile.getBlockState()))
                  .append(",\"powered\":").append(tile.isPowered())
                  .append(",\"comparator\":").append(tile.getCurrentRedstoneLevel())
                  .append(",\"max_operations\":").append(tile.getBaselineMaxOperations())
                  .append(",\"hydrogen\":").append(tile.isMakingHydrogen())
                  .append(",\"water\":").append(tile.fluidTank.amountAsLong())
                  .append(",\"left\":").append(tile.leftTank.amountAsLong())
                  .append(",\"right\":").append(tile.rightTank.amountAsLong())
                  .append(",\"errors\":[");
            List<RecipeError> errors = List.of(RecipeError.NOT_ENOUGH_ENERGY, RecipeError.NOT_ENOUGH_ENERGY_REDUCED_RATE, RecipeError.NOT_ENOUGH_INPUT,
                  TileEntityElectrolyticSeparator.NOT_ENOUGH_SPACE_LEFT_OUTPUT_ERROR, TileEntityElectrolyticSeparator.NOT_ENOUGH_SPACE_RIGHT_OUTPUT_ERROR,
                  RecipeError.INPUT_DOESNT_PRODUCE_OUTPUT);
            List<String> names = List.of("energy", "reduced_rate", "input", "left_space", "right_space", "wrong_output");
            boolean first = true;
            for (int i = 0; i < errors.size(); i++) {
                if (tile.getWarningCheck(errors.get(i)).getAsBoolean()) {
                    line.append(first ? "" : ",").append('"').append(names.get(i)).append('"');
                    first = false;
                }
            }
            line.append(']');
            appendExtras(line);
            lines.add(line.append('}').toString());
            refill();
        }

        void record() {
            if (helper == null) {
                return;
            }
            if (separator) {
                recordSeparator();
                return;
            }
            TileEntityProgressMachine<?> chamber = machine();
            MachineEnergyContainer<?> energy = energy();
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++).append(",\"marks\":[");
            for (int i = 0; i < marks.size(); i++) {
                line.append(i == 0 ? "" : ",").append('"').append(marks.get(i)).append('"');
            }
            marks.clear();
            line.append("],\"progress\":").append(chamber.getRecipeProgress())
                  .append(",\"required\":").append(chamber.getTicksRequired())
                  .append(",\"energy\":").append(energy.getAmountAsLong())
                  .append(",\"capacity\":").append(energy.getCapacityAsLong())
                  .append(",\"per_tick\":").append(energy.getEnergyPerTick())
                  .append(",\"active\":").append(chamber.getActive())
                  .append(",\"block_active\":").append(Attribute.isActive(chamber.getBlockState()))
                  .append(",\"powered\":").append(chamber.isPowered())
                  .append(",\"comparator\":").append(chamber.getCurrentRedstoneLevel())
                  .append(",\"input\":[");
            slot(line, chamber.getInventorySlots().get(inputSlot));
            line.append("],\"output\":[");
            slot(line, chamber.getInventorySlots().get(outputSlot));
            line.append("],\"errors\":[");
            boolean first = true;
            for (int i = 0; i < ERRORS.size(); i++) {
                if (chamber.getWarningCheck(ERRORS.get(i)).getAsBoolean()) {
                    line.append(first ? "" : ",").append('"').append(ERROR_NAMES.get(i)).append('"');
                    first = false;
                }
            }
            line.append(']');
            if (chamber instanceof TileEntityMetallurgicInfuser infuser) {
                line.append(",\"tank\":").append(infuser.infusionTank.amountAsLong());
                line.append(",\"infusion\":[");
                slot(line, chamber.getInventorySlots().get(0));
                line.append(']');
            }
            if (recordUpgrades) {
                line.append(",\"speed\":").append(chamber.getComponent().getUpgrades(upgrade(UpgradeIds.SPEED)))
                      .append(",\"energy_upgrades\":").append(chamber.getComponent().getUpgrades(upgrade(UpgradeIds.ENERGY)))
                      .append(",\"upgrade_progress\":").append(chamber.getComponent().getScaledUpgradeProgress())
                      .append(",\"upgrade_in\":[");
                upgradeSlot(line, chamber.getComponent().getUpgradeSlot());
                line.append("],\"upgrade_out\":[");
                upgradeSlot(line, chamber.getComponent().getUpgradeOutputSlot());
                line.append(']');
            }
            if (recordChest) {
                line.append(",\"chest\":[");
                boolean firstSlot = true;
                for (int i = 0; i < chest().getContainerSize(); i++) {
                    ItemStack stack = chest().getItem(i);
                    if (!stack.isEmpty()) {
                        line.append(firstSlot ? "" : ",").append('[').append(i).append(",\"")
                              .append(BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath()).append("\",").append(stack.getCount()).append(']');
                        firstSlot = false;
                    }
                }
                line.append(']');
            }
            appendExtras(line);
            lines.add(line.append('}').toString());
            refill();
        }

        void write() {
            helper = null;
            try {
                Path file = Path.of("trace", name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }
}
