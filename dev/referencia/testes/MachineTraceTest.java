package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.lang.reflect.Field;
import java.lang.reflect.Proxy;
import java.util.List;
import java.util.Map;
import java.util.Comparator;
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
import mekanism.common.block.attribute.AttributeEnergy;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.config.MekanismConfig;
import mekanism.common.inventory.container.MekanismContainer;
import mekanism.common.inventory.container.SelectedWindowData;
import mekanism.common.inventory.container.SelectedWindowData.WindowType;
import mekanism.common.inventory.container.sync.ISyncableData;
import mekanism.common.inventory.container.tile.MekanismTileContainer;
import mekanism.common.inventory.warning.WarningTracker;
import mekanism.common.inventory.warning.WarningTracker.WarningType;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.network.to_server.PacketGuiInteract;
import mekanism.common.network.to_server.PacketGuiInteract.GuiInteraction;
import mekanism.common.network.to_server.PacketRemoveUpgrade;
import mekanism.common.network.to_server.PacketWindowSelect;
import mekanism.common.registration.impl.ContainerTypeRegistryObject;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismContainerTypes;
import mekanism.common.registries.MekanismItems;
import mekanism.common.resource.PrimaryResource;
import mekanism.common.resource.ResourceType;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.component.TileComponentUpgrade;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.component.TileComponentUpgrade;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.tile.interfaces.IRedstoneControl.RedstoneControl;
import mekanism.common.tile.machine.TileEntityElectrolyticSeparator;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import mekanism.common.tile.prefab.TileEntityAdvancedElectricMachine;
import mekanism.common.tile.prefab.TileEntityElectricMachine;
import mekanism.common.tile.prefab.TileEntityProgressMachine;
import mekanism.common.tile.prefab.TileEntityRecipeMachine;
import mekanism.api.chemical.ChemicalStack;
import mekanism.api.recipes.ItemStackChemicalToItemStackRecipe;
import mekanism.api.recipes.ItemStackToItemStackRecipe;
import mekanism.api.recipes.MekanismRecipe;
import mekanism.api.recipes.basic.BasicEnrichingRecipe;
import mekanism.api.recipes.basic.BasicMetallurgicInfuserRecipe;
import mekanism.api.recipes.book.EnrichingBookCategory;
import mekanism.api.recipes.book.EnrichingBookCategory.EnrichingBookInfo;
import mekanism.api.recipes.book.MetallurgicInfusingBookCategory;
import mekanism.api.recipes.book.MetallurgicInfusingBookCategory.MetallurgicInfusingBookInfo;
import mekanism.api.recipes.ingredients.ChemicalStackIngredient;
import mekanism.api.recipes.ingredients.ItemStackIngredient;
import mekanism.api.recipes.ingredients.creator.IngredientCreatorAccess;
import mekanism.common.recipe.MekanismRecipeType;
import mekanism.common.recipe.lookup.cache.InputRecipeCache;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.Registries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.ItemStackTemplate;
import net.minecraft.world.item.crafting.Ingredient;
import net.minecraft.world.item.crafting.Recipe;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.neoforged.neoforge.common.crafting.CompoundIngredient;
import net.neoforged.neoforge.common.crafting.DataComponentIngredient;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.ResourceKey;
import net.minecraft.network.chat.Component;
import net.minecraft.world.Container;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.network.handling.IPayloadContext;
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
    @TestHolder(description = "Records the Osmium Compressor, whose recipe spends the osmium every tick: a tank that runs dry and pauses the recipe, a speed upgrade that makes the spending catch up, and the energy running out.")
    public static void compressorRun(final DynamicTest test) {
        template(test, MekanismBlocks.OSMIUM_COMPRESSOR::defaultState);
        Trace trace = new Trace("compressor_run");
        trace.inputSlot = 0;
        trace.outputSlot = 2;
        trace.refillTicks = 3;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("item glowstone_dust 3, osmium 210");
                  trace.setSlot(0, Items.GLOWSTONE_DUST, 3);
                  trace.setOsmium(210);
              })
              .thenExecuteAfter(235, () -> {
                  trace.mark("osmium 100");
                  trace.setOsmium(100);
              })
              .thenExecuteAfter(120, () -> {
                  trace.mark("speed 4, osmium 210");
                  trace.addUpgrades(UpgradeIds.SPEED, 4);
                  trace.setOsmium(210);
              })
              .thenExecuteAfter(40, () -> {
                  trace.mark("osmium 0");
                  trace.setOsmium(0);
              })
              .thenExecuteAfter(15, () -> {
                  trace.mark("osmium 100");
                  trace.setOsmium(100);
              })
              .thenExecuteAfter(15, () -> {
                  trace.mark("energy 0, no top ups");
                  trace.refillTicks = 0;
                  trace.setEnergy(0);
              })
              .thenExecuteAfter(10, () -> {
                  trace.mark("top ups back");
                  trace.refillTicks = 3;
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("gas slot ingot_osmium 2, osmium 5");
                  trace.setSlot(1, MekanismItems.getProcessedResource(ResourceType.INGOT, PrimaryResource.OSMIUM).asItem(), 2);
                  trace.setOsmium(5);
              })
              .thenExecuteAfter(30, trace::write)
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

    /// Tabela de upgrades: quatro máquinas lado a lado, cada uma com a mesma sequência de mudanças. Para cada energia de 0 a 8,
    /// a velocidade sobe de 0 a 8 um por um. Depois de cada mudança a máquina processa alguns ticks e o estado é lido dos getters.
    /// Depois, a Enrichment Chamber roda com energia cheia e grava o quanto o estoque cai por tick.
    private static final List<String> UPGRADE_MACHINES = List.of("enrichment_chamber", "crusher", "energized_smelter", "metallurgic_infuser");
    private static final List<BlockPos> UPGRADE_POSITIONS = List.of(new BlockPos(1, 0, 0), new BlockPos(3, 0, 0), new BlockPos(5, 0, 0), new BlockPos(7, 0, 0));
    private static final List<Item> UPGRADE_INPUTS = List.of(Items.CLAY, Items.COBBLESTONE, Items.RAW_IRON, Items.COPPER_INGOT);
    /// A Metallurgic Infuser tem o slot de infusão antes da entrada, por isso a entrada e a saída dela são 1 e 2.
    private static final int[] UPGRADE_INPUT_SLOTS = {0, 0, 0, 1};
    private static final int[] UPGRADE_OUTPUT_SLOTS = {1, 1, 1, 2};
    private static final int UPGRADE_SETTLE_TICKS = 3;
    private static final int CONSUMPTION_TICKS = 12;
    private static final int UPGRADE_TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = UPGRADE_TIMEOUT, batch = "trace_upgrade_table")
    @TestHolder(description = "Records ticks required, energy per tick, capacity and stored energy of four machines for each pair of energy and speed upgrades, and the energy the chamber spends per tick.")
    public static void upgradeTable(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(8, 1, 1)
              .fill(1, 0, 0, 1, 0, 0, MekanismBlocks.ENRICHMENT_CHAMBER.defaultState())
              .fill(3, 0, 0, 3, 0, 0, MekanismBlocks.CRUSHER.defaultState())
              .fill(5, 0, 0, 5, 0, 0, MekanismBlocks.ENERGIZED_SMELTER.defaultState())
              .fill(7, 0, 0, 7, 0, 0, MekanismBlocks.METALLURGIC_INFUSER.defaultState()));
        UpgradeTable table = new UpgradeTable();
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> table.sample());
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var seq = helper.startSequence().thenExecute(() -> table.start(helper));
            for (int energy = 0; energy <= 8; energy++) {
                int e = energy;
                seq = seq.thenExecuteAfter(2, () -> table.reset(e))
                      .thenExecuteAfter(UPGRADE_SETTLE_TICKS, () -> table.record(e, 0));
                for (int speed = 1; speed <= 8; speed++) {
                    int s = speed;
                    seq = seq.thenExecuteAfter(1, table::speedUp)
                          .thenExecuteAfter(UPGRADE_SETTLE_TICKS, () -> table.record(e, s));
                }
            }
            for (int energy : List.of(0, 4, 8)) {
                int e = energy;
                seq = seq.thenExecuteAfter(2, () -> table.reset(e))
                      .thenExecuteAfter(1, table::startCapture)
                      .thenExecuteAfter(CONSUMPTION_TICKS, () -> table.endCapture(e));
            }
            seq.thenExecuteAfter(2, table::write).thenSucceed();
        });
    }

    /// Estado da tabela de upgrades. Lê as máquinas pelo teste e acumula as linhas de `upgrade_table.jsonl`.
    private static class UpgradeTable {

        private final List<String> lines = new ArrayList<>();
        private final List<String> samples = new ArrayList<>();
        private MekGameTestHelper helper;
        private boolean capturing;
        private long capturedCapacity;

        void start(MekGameTestHelper helper) {
            this.helper = helper;
            lines.add("{\"meta\":\"upgrade_max\",\"energy\":" + upgrade(UpgradeIds.ENERGY).value().max()
                  + ",\"speed\":" + upgrade(UpgradeIds.SPEED).value().max() + "}");
            lines.add("{\"meta\":\"config\",\"max_upgrade_multiplier\":" + MekanismConfig.general.maxUpgradeMultiplier.get()
                  + ",\"usage\":{\"enrichment_chamber\":" + MekanismConfig.usage.enrichmentChamber.get()
                  + ",\"crusher\":" + MekanismConfig.usage.crusher.get()
                  + ",\"energized_smelter\":" + MekanismConfig.usage.energizedSmelter.get()
                  + ",\"metallurgic_infuser\":" + MekanismConfig.usage.metallurgicInfuser.get() + "}}");
            for (int i = 0; i < UPGRADE_MACHINES.size(); i++) {
                AttributeEnergy attribute = Attribute.getOrThrow(tile(i).getBlockHolder(), AttributeEnergy.class);
                MachineEnergyContainer<?> energy = energy(i);
                lines.add("{\"meta\":\"storage\",\"machine\":\"" + UPGRADE_MACHINES.get(i) + "\",\"usage\":" + attribute.getUsage()
                      + ",\"config_storage\":" + attribute.getConfigStorage() + ",\"storage\":" + attribute.getStorage()
                      + ",\"base_max_energy\":" + energy.getBaseMaxEnergy() + ",\"base_energy_per_tick\":" + energy.getBaseEnergyPerTick()
                      + ",\"base_ticks_required\":" + progress(i).getTicksRequired() + "}");
            }
        }

        private Holder<Upgrade> upgrade(ResourceKey<Upgrade> key) {
            return helper.getLevel().registryAccess().lookupOrThrow(MekanismRegistries.Keys.UPGRADES).getOrThrow(key);
        }

        private TileEntityRecipeMachine<?, ?> tile(int i) {
            BlockEntity tile = helper.getLevel().getBlockEntity(helper.absolutePos(UPGRADE_POSITIONS.get(i)));
            return (TileEntityRecipeMachine<?, ?>) tile;
        }

        private TileEntityProgressMachine<?> progress(int i) {
            return (TileEntityProgressMachine<?>) tile(i);
        }

        private MachineEnergyContainer<?> energy(int i) {
            return (MachineEnergyContainer<?>) tile(i).getEnergyContainer();
        }

        /// Lê por reflexão o `currentMaxEnergy` do container: a capacidade que a máquina usa. O getter `getCapacityAsLong` devolve
        /// o maior entre ela e o que está guardado, então um estoque acima da capacidade nova apareceria como capacidade.
        private static long currentMaxEnergy(MachineEnergyContainer<?> energy) {
            try {
                Field field = MachineEnergyContainer.class.getDeclaredField("currentMaxEnergy");
                field.setAccessible(true);
                return field.getLong(energy);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        /// Enche o container com a capacidade atual (zera antes, para não herdar um estoque antigo acima da capacidade nova).
        private void fill(int i) {
            MachineEnergyContainer<?> energy = energy(i);
            energy.setEnergy(0, null);
            energy.setEnergy(currentMaxEnergy(energy), null);
        }

        /// Tira todos os upgrades (pelo caminho real de remoção, que manda a saída para o slot de saída), limpa a saída e
        /// repõe entrada, tanque e energia. Depois instala a energia pedida de uma vez.
        void reset(int energy) {
            for (int i = 0; i < UPGRADE_MACHINES.size(); i++) {
                TileEntityRecipeMachine<?, ?> tile = tile(i);
                TileComponentUpgrade component = tile.getComponent();
                component.removeUpgrade(helper.getLevel().registryAccess(), upgrade(UpgradeIds.ENERGY), true);
                component.getUpgradeOutputSlot().setContents(ItemResource.EMPTY, 0, null);
                component.removeUpgrade(helper.getLevel().registryAccess(), upgrade(UpgradeIds.SPEED), true);
                component.getUpgradeOutputSlot().setContents(ItemResource.EMPTY, 0, null);
                if (component.getUpgrades(upgrade(UpgradeIds.ENERGY)) != 0 || component.getUpgrades(upgrade(UpgradeIds.SPEED)) != 0) {
                    throw new IllegalStateException("Upgrades were not removed from " + UPGRADE_MACHINES.get(i));
                }
                //A remoção só recalcula quando havia upgrade. Com zero instalado, recalcula como o carregamento do tile faz (deserialize),
                // para que a base seja a da máquina pronta (capacidade de speed 0), e não a do construtor.
                tile.recalculateUpgrades(tile.getUpgradeLookup(helper.getLevel().registryAccess()), upgrade(UpgradeIds.ENERGY), 0);
                tile.recalculateUpgrades(tile.getUpgradeLookup(helper.getLevel().registryAccess()), upgrade(UpgradeIds.SPEED), 0);
                TileEntityProgressMachine<?> machine = progress(i);
                machine.getInventorySlots().get(UPGRADE_OUTPUT_SLOTS[i]).setContents(ItemResource.EMPTY, 0, null);
                machine.getInventorySlots().get(UPGRADE_INPUT_SLOTS[i]).setContents(ItemResource.of(UPGRADE_INPUTS.get(i)), 64, null);
                if (tile instanceof TileEntityMetallurgicInfuser infuser) {
                    Holder<Chemical> redstone = helper.getLevel().registryAccess().getOrThrow(ChemicalIds.REDSTONE);
                    infuser.infusionTank.setContents(ChemicalResource.of(redstone), 30, null);
                }
                if (energy > 0) {
                    component.addUpgrades(helper.getLevel().registryAccess(), upgrade(UpgradeIds.ENERGY), energy);
                }
                fill(i);
            }
        }

        /// Sobe a velocidade em 1 em cada máquina e enche o container com a capacidade nova.
        void speedUp() {
            for (int i = 0; i < UPGRADE_MACHINES.size(); i++) {
                tile(i).getComponent().addUpgrades(helper.getLevel().registryAccess(), upgrade(UpgradeIds.SPEED), 1);
                fill(i);
            }
        }

        /// Lê o estado de cada máquina. Confere antes que os upgrades do componente são os esperados.
        void record(int energyLevel, int speedLevel) {
            for (int i = 0; i < UPGRADE_MACHINES.size(); i++) {
                TileEntityProgressMachine<?> machine = progress(i);
                MachineEnergyContainer<?> energy = energy(i);
                int energyUpgrades = machine.getComponent().getUpgrades(upgrade(UpgradeIds.ENERGY));
                int speedUpgrades = machine.getComponent().getUpgrades(upgrade(UpgradeIds.SPEED));
                if (energyUpgrades != energyLevel || speedUpgrades != speedLevel) {
                    throw new IllegalStateException("Expected energy " + energyLevel + " and speed " + speedLevel + " in "
                          + UPGRADE_MACHINES.get(i) + ", found " + energyUpgrades + " and " + speedUpgrades);
                }
                lines.add("{\"machine\":\"" + UPGRADE_MACHINES.get(i) + "\",\"energy_upgrades\":" + energyUpgrades
                      + ",\"speed_upgrades\":" + speedUpgrades + ",\"max_energy\":" + currentMaxEnergy(energy)
                      + ",\"energy_per_tick\":" + energy.getEnergyPerTick() + ",\"ticks_required\":" + machine.getTicksRequired()
                      + ",\"stored\":" + energy.getAmountAsLong() + ",\"capacity_getter\":" + energy.getCapacityAsLong() + "}");
            }
        }

        void startCapture() {
            samples.clear();
            capturedCapacity = currentMaxEnergy(energy(0));
            capturing = true;
        }

        /// Chamada a cada tick do servidor, depois do tick das máquinas: grava [energia, progresso] da Enrichment Chamber.
        void sample() {
            if (capturing) {
                samples.add("[" + energy(0).getAmountAsLong() + "," + progress(0).getRecipeProgress() + "]");
            }
        }

        void endCapture(int energyLevel) {
            capturing = false;
            lines.add("{\"meta\":\"consumption\",\"machine\":\"enrichment_chamber\",\"energy_upgrades\":" + energyLevel
                  + ",\"speed_upgrades\":0,\"energy_per_tick\":" + energy(0).getEnergyPerTick() + ",\"capacity\":" + capturedCapacity
                  + ",\"samples\":[" + String.join(",", samples) + "]}");
        }

        void write() {
            try {
                Path file = Path.of("trace", "upgrade_table.jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the upgrade window as a player uses it: window selected and tracked by the real packets, upgrades installed by clicking the virtual slot.")
    public static void chamberUpgradeWindowInstall(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_upgrade_window_install");
        trace.recordUpgrades = true;
        trace.recordUpgradeWindow = true;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("open container");
                  trace.openMenu(MekanismContainerTypes.ENRICHMENT_CHAMBER);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("select upgrade window, track upgrades");
                  trace.selectWindow(new SelectedWindowData(WindowType.UPGRADE));
                  trace.interact(GuiInteraction.CONTAINER_TRACK_UPGRADES);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("click upgrade slot with speed 3");
                  trace.clickUpgradeSlot(UpgradeIds.SPEED, 3);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("click upgrade slot with speed 10");
                  trace.clickUpgradeSlot(UpgradeIds.SPEED, 10);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("click upgrade slot with energy 2");
                  trace.clickUpgradeSlot(UpgradeIds.ENERGY, 2);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("drop what is carried");
                  trace.dropCarried();
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("stop tracking, deselect window");
                  trace.interact(GuiInteraction.CONTAINER_STOP_TRACKING);
                  trace.selectWindow(null);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("click upgrade slot with energy 1, no window");
                  trace.clickUpgradeSlot(UpgradeIds.ENERGY, 1);
              })
              .thenExecuteAfter(30, () -> {
                  trace.mark("close container");
                  trace.closeMenu();
              })
              .thenExecuteAfter(5, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the real remove upgrade packet, one and all, with an empty output slot, a full one, one with another type and one with room for part.")
    public static void chamberUpgradeRemoveByPacket(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_upgrade_remove_by_packet");
        trace.recordUpgrades = true;
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> trace.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> trace.start(helper))
              .thenExecuteAfter(5, () -> {
                  trace.mark("install speed 5, energy 4");
                  trace.addUpgrades(UpgradeIds.SPEED, 5);
                  trace.addUpgrades(UpgradeIds.ENERGY, 4);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("packet remove speed one, output empty");
                  trace.removePacket(UpgradeIds.SPEED, false);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("packet remove speed all, output holds speed 1");
                  trace.removePacket(UpgradeIds.SPEED, true);
              })
              .thenExecuteAfter(5, () -> {
                  int full = trace.upgradeStack(UpgradeIds.SPEED, 1).getMaxStackSize();
                  trace.mark("install speed 8, output speed " + full);
                  trace.addUpgrades(UpgradeIds.SPEED, 8);
                  trace.setUpgradeOutput(UpgradeIds.SPEED, full);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("packet remove speed one, output full");
                  trace.removePacket(UpgradeIds.SPEED, false);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("packet remove speed all, output full");
                  trace.removePacket(UpgradeIds.SPEED, true);
              })
              .thenExecuteAfter(5, () -> {
                  int room = trace.upgradeStack(UpgradeIds.ENERGY, 1).getMaxStackSize() - 2;
                  trace.mark("output energy " + room + " with room for 2");
                  trace.setUpgradeOutput(UpgradeIds.ENERGY, room);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("packet remove speed one, output holds another type");
                  trace.removePacket(UpgradeIds.SPEED, false);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("packet remove speed all, output holds another type");
                  trace.removePacket(UpgradeIds.SPEED, true);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("packet remove energy all, installed 4 and room for 2");
                  trace.removePacket(UpgradeIds.ENERGY, true);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("packet remove energy one, output now full");
                  trace.removePacket(UpgradeIds.ENERGY, false);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("output cleared, packet remove energy all");
                  trace.clearUpgradeSlot();
                  trace.setUpgradeOutput(UpgradeIds.ENERGY, 0);
                  trace.removePacket(UpgradeIds.ENERGY, true);
              })
              .thenExecuteAfter(5, () -> {
                  trace.mark("packet remove xp_containment, not installed");
                  trace.removePacket(UpgradeIds.XP_CONTAINMENT, false);
              })
              .thenExecuteAfter(10, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records quickMoveStack on the upgrade slots with the upgrade window selected and without it, going in and coming out.")
    public static void chamberUpgradeSlotQuickMove(final DynamicTest test) {
        template(test);
        Trace trace = new Trace("chamber_upgrade_slot_quick_move");
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  trace.start(helper);
                  trace.openMenu(MekanismContainerTypes.ENRICHMENT_CHAMBER);
                  trace.layout();
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("no window: upgrade speed 3 from main slot 9");
                  trace.quickMoveFromPlayer(9, trace.upgradeStack(UpgradeIds.SPEED, 3));
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("no window: clay 10 from main slot 10");
                  trace.quickMoveFromPlayer(10, new ItemStack(Items.CLAY, 10));
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("select upgrade window");
                  trace.selectWindow(new SelectedWindowData(WindowType.UPGRADE));
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("window: upgrade speed 3 from main slot 11");
                  trace.quickMoveFromPlayer(11, trace.upgradeStack(UpgradeIds.SPEED, 3));
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("window: upgrade energy 2 from main slot 12, input slot already has speed");
                  trace.quickMoveFromPlayer(12, trace.upgradeStack(UpgradeIds.ENERGY, 2));
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("window: upgrade the machine does not support from main slot 13");
                  trace.quickMoveFromPlayer(13, trace.unsupportedUpgradeStack());
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("window: clay 10 from main slot 14");
                  trace.quickMoveFromPlayer(14, new ItemStack(Items.CLAY, 10));
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("window: upgrade speed 3 from hotbar slot 0");
                  trace.quickMoveFromPlayer(0, trace.upgradeStack(UpgradeIds.SPEED, 3));
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("window: from the virtual input slot back to the inventory");
                  trace.quickMoveFromMenu(0);
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("window: output slot holds energy 3, quick move it out");
                  trace.setUpgradeOutput(UpgradeIds.ENERGY, 3);
                  trace.quickMoveFromMenu(1);
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("deselect window, input slot holds speed 2");
                  trace.selectWindow(null);
                  trace.setUpgradeSlot(UpgradeIds.SPEED, 2);
                  trace.snapshot();
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("no window: quick move the virtual input slot");
                  trace.quickMoveFromMenu(0);
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("no window: output slot holds energy 3, quick move it");
                  trace.setUpgradeOutput(UpgradeIds.ENERGY, 3);
                  trace.quickMoveFromMenu(1);
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("machine slots: quick move the clay in the input slot out");
                  trace.quickMoveFromMenu(2);
              })
              .thenExecuteAfter(2, trace::write)
              .thenSucceed()
        );
    }

    private static void containerTracking(DynamicTest test, String name, ContainerTypeRegistryObject<?> type) {
        Trace trace = new Trace(name);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  trace.start(helper);
                  trace.mark("container created");
                  trace.openMenu(type);
                  trace.trackingLine();
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("PacketWindowSelect upgrade");
                  trace.selectWindow(new SelectedWindowData(WindowType.UPGRADE));
                  trace.trackingLine();
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("PacketGuiInteract CONTAINER_TRACK_UPGRADES");
                  trace.interact(GuiInteraction.CONTAINER_TRACK_UPGRADES);
                  trace.trackingLine();
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("PacketGuiInteract CONTAINER_STOP_TRACKING");
                  trace.interact(GuiInteraction.CONTAINER_STOP_TRACKING);
                  trace.trackingLine();
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("PacketGuiInteract CONTAINER_TRACK_UPGRADES again");
                  trace.interact(GuiInteraction.CONTAINER_TRACK_UPGRADES);
                  trace.trackingLine();
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("PacketGuiInteract CONTAINER_TRACK_UPGRADES a second time, without stopping");
                  trace.interact(GuiInteraction.CONTAINER_TRACK_UPGRADES);
                  trace.trackingLine();
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("PacketWindowSelect none");
                  trace.selectWindow(null);
                  trace.trackingLine();
              })
              .thenExecuteAfter(2, () -> {
                  trace.mark("container closed");
                  trace.closeMenu();
                  trace.trackingLine();
              })
              .thenExecuteAfter(2, trace::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the order and class of the data the Enrichment Chamber container tracks, and what the upgrade window adds and removes.")
    public static void chamberContainerTracking(final DynamicTest test) {
        template(test);
        containerTracking(test, "chamber_container_tracking", MekanismContainerTypes.ENRICHMENT_CHAMBER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records the same for the Metallurgic Infuser container.")
    public static void infuserContainerTracking(final DynamicTest test) {
        template(test, MekanismBlocks.METALLURGIC_INFUSER::defaultState);
        containerTracking(test, "infuser_container_tracking", MekanismContainerTypes.METALLURGIC_INFUSER);
    }

    /// Grava o que os caches de receitas respondem (`InputRecipeCache`), para o teste em Rust comparar. Cada linha é um cenário:
    /// as receitas na ordem da lista e as perguntas com a resposta. O cache é montado com `initCache` sobre a lista, como o
    /// `initCacheIfNeeded` faz com `recipeType.getRecipes`.
    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_machine")
    @TestHolder(description = "Records which recipe the input caches find, where the list order and the per item map disagree.")
    public static void recipeCacheOrder(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> RecipeCacheTrace.run(helper))
              .thenSucceed()
        );
    }

    private static class RecipeCacheTrace {

        private static final String SINGLE = "single";
        private static final String DOUBLE = "double";

        /// Um ingrediente de item: `simple` (vanilla), `exact` (componentes exatos), `partial` (componentes parciais, complexo) ou `compound`.
        private record Spec(String kind, List<String> items, Integer repairCost, List<Spec> children) {

            static Spec simple(String... items) {
                return new Spec("simple", List.of(items), null, List.of());
            }

            static Spec exact(String item, Integer repairCost) {
                return new Spec("exact", List.of(item), repairCost, List.of());
            }

            static Spec partial(String item, int repairCost) {
                return new Spec("partial", List.of(item), repairCost, List.of());
            }

            static Spec compound(Spec... children) {
                return new Spec("compound", List.of(), null, List.of(children));
            }

            Item item(String name) {
                return BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(name));
            }

            Ingredient ingredient() {
                return switch (kind) {
                    case "simple" -> Ingredient.of(items.stream().map(this::item).toArray(Item[]::new));
                    case "exact" -> {
                        ItemStack stack = new ItemStack(item(items.get(0)));
                        if (repairCost != null) {
                            stack.set(DataComponents.REPAIR_COST, repairCost);
                        }
                        yield DataComponentIngredient.of(true, stack);
                    }
                    case "partial" -> DataComponentIngredient.of(false, DataComponentPatch.builder().set(DataComponents.REPAIR_COST, repairCost).build(),
                          items.stream().map(this::item).toArray(Item[]::new));
                    default -> CompoundIngredient.of(children.stream().map(Spec::ingredient).toArray(Ingredient[]::new));
                };
            }

            String json() {
                StringBuilder out = new StringBuilder("{\"kind\":\"").append(kind).append("\",\"items\":[");
                for (int i = 0; i < items.size(); i++) {
                    out.append(i == 0 ? "" : ",").append('"').append(items.get(i)).append('"');
                }
                out.append("],\"rc\":").append(repairCost == null ? "null" : repairCost).append(",\"children\":[");
                for (int i = 0; i < children.size(); i++) {
                    out.append(i == 0 ? "" : ",").append(children.get(i).json());
                }
                return out.append("]}").toString();
            }
        }

        /// Uma entrada: item (`""` é vazio), componente `repair_cost` e quantidade; ou o químico.
        private record Stack(String id, Integer repairCost, int amount) {

            ItemStack item() {
                if (id.isEmpty()) {
                    return ItemStack.EMPTY;
                }
                ItemStack stack = new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(id)), amount);
                if (repairCost != null) {
                    stack.set(DataComponents.REPAIR_COST, repairCost);
                }
                return stack;
            }

            String json() {
                return "{\"id\":\"" + id + "\",\"rc\":" + (repairCost == null ? "null" : repairCost) + ",\"amount\":" + amount + "}";
            }
        }

        private static Stack item(String id, Integer repairCost, int amount) {
            return new Stack(id, repairCost, amount);
        }

        private static Stack chemical(String id, int amount) {
            return new Stack(id, null, amount);
        }

        private static final Stack NONE = new Stack("", null, 0);

        private static String quote(String text) {
            return text == null ? "null" : "\"" + text + "\"";
        }

        private record SingleRecipe(String id, Spec input, int count) {
        }

        private record DoubleRecipe(String id, Spec item, int count, String chemical, int amount) {
        }

        private static final List<String> lines = new ArrayList<>();

        static void run(MekGameTestHelper helper) {
            lines.clear();
            registry = helper.getLevel().registryAccess();
            single("complex_before_simple", List.of(
                        new SingleRecipe("c0", Spec.partial("coal", 5), 1),
                        new SingleRecipe("s1", Spec.simple("coal"), 1)),
                  List.of(item("coal", 5, 1), item("coal", null, 1), item("coal", 6, 1), item("diamond", null, 1), NONE));
            single("exact_before_simple", List.of(
                        new SingleRecipe("s0", Spec.simple("sand"), 3),
                        new SingleRecipe("e1", Spec.exact("sand", 5), 1),
                        new SingleRecipe("s2", Spec.simple("sand"), 1)),
                  List.of(item("sand", 5, 5), item("sand", 5, 1), item("sand", null, 5), item("sand", null, 1), item("sand", 6, 5), item("sand", 6, 1)));
            single("exact_plain_components", List.of(
                        new SingleRecipe("s0", Spec.simple("sand"), 1),
                        new SingleRecipe("e1", Spec.exact("sand", null), 1)),
                  List.of(item("sand", null, 1), item("sand", 5, 1)));
            single("complex_only", List.of(new SingleRecipe("p0", Spec.partial("diamond", 5), 1)),
                  List.of(item("diamond", 5, 1), item("diamond", null, 1), item("coal", null, 1)));
            single("compound_complex_before_simple", List.of(
                        new SingleRecipe("m0", Spec.compound(Spec.partial("diamond", 5), Spec.simple("charcoal")), 1),
                        new SingleRecipe("s1", Spec.simple("diamond"), 1)),
                  List.of(item("diamond", 5, 1), item("diamond", null, 1), item("charcoal", null, 1), item("charcoal", 5, 1)));
            single("several_items_and_counts", List.of(
                        new SingleRecipe("r0", Spec.simple("coal", "charcoal"), 2),
                        new SingleRecipe("r1", Spec.simple("coal"), 1)),
                  List.of(item("charcoal", null, 1), item("charcoal", null, 2), item("coal", null, 1), item("coal", null, 2)));
            Spec rc5 = Spec.partial("coal", 5);
            Spec coal = Spec.simple("coal");
            Spec sand = Spec.simple("sand");
            doubles("complex_item_before_simple", List.of(
                        new DoubleRecipe("d0", rc5, 1, "redstone", 10),
                        new DoubleRecipe("d1", coal, 1, "redstone", 10)),
                  List.of(item("coal", 5, 1), item("coal", null, 1), item("coal", 6, 1)),
                  List.of(chemical("redstone", 10), chemical("redstone", 9), chemical("carbon", 10), NONE));
            doubles("simple_pairs", List.of(
                        new DoubleRecipe("d0", coal, 1, "redstone", 10),
                        new DoubleRecipe("d1", sand, 1, "carbon", 10)),
                  List.of(item("coal", null, 1), item("sand", null, 1), item("diamond", null, 1), NONE),
                  List.of(chemical("redstone", 10), chemical("carbon", 10), chemical("diamond", 10), NONE));
            doubles("complex_item_pairs", List.of(new DoubleRecipe("d0", rc5, 1, "redstone", 10)),
                  List.of(item("coal", 5, 1), item("coal", null, 1), NONE),
                  List.of(chemical("redstone", 10), chemical("carbon", 10), NONE));
            doubles("exact_item_first", List.of(
                        new DoubleRecipe("d0", sand, 3, "redstone", 10),
                        new DoubleRecipe("d1", Spec.exact("sand", 5), 1, "redstone", 10),
                        new DoubleRecipe("d2", sand, 1, "redstone", 10)),
                  List.of(item("sand", 5, 5), item("sand", null, 1), item("sand", 6, 5)),
                  List.of(chemical("redstone", 10), chemical("redstone", 20)));
            try {
                Path file = Path.of("trace", "recipe_cache.jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }

        private static Recipe.CommonInfo common() {
            return new Recipe.CommonInfo(true);
        }

        private static String join(List<String> parts) {
            return "[" + String.join(",", parts) + "]";
        }

        private static void single(String name, List<SingleRecipe> recipes, List<Stack> stacks) {
            List<RecipeHolder<ItemStackToItemStackRecipe<EnrichingBookCategory, EnrichingBookInfo>>> holders = new ArrayList<>();
            List<String> recipeJson = new ArrayList<>();
            for (SingleRecipe recipe : recipes) {
                ItemStackIngredient ingredient = IngredientCreatorAccess.item().from(recipe.input.ingredient(), recipe.count);
                holders.add(new RecipeHolder<>(ResourceKey.create(Registries.RECIPE, Identifier.fromNamespaceAndPath("trace", recipe.id)),
                      new BasicEnrichingRecipe(common(), new EnrichingBookInfo(EnrichingBookCategory.MISC, ""), ingredient, 0F, new ItemStackTemplate(Items.STONE))));
                recipeJson.add("{\"id\":\"" + recipe.id + "\",\"input\":" + recipe.input.json() + ",\"count\":" + recipe.count + "}");
            }
            InputRecipeCache.SingleItem<ItemStackToItemStackRecipe<EnrichingBookCategory, EnrichingBookInfo>> cache = new InputRecipeCache.SingleItem<>(
                  MekanismRecipeType.ENRICHING.get(), ItemStackToItemStackRecipe::getInput) {
                {
                    initCache(holders);
                    initialized = true;
                }
            };
            List<String> queries = new ArrayList<>();
            for (Stack stack : stacks) {
                ItemStack itemStack = stack.item();
                ItemResource resource = ItemResource.of(itemStack);
                String head = "{\"stack\":" + stack.json();
                queries.add(head + ",\"op\":\"find_first\",\"result\":" + quote(idOf(holders, cache.findFirstRecipe(null, itemStack))) + "}");
                if (!itemStack.isEmpty()) {
                    queries.add(head + ",\"op\":\"contains\",\"result\":" + cache.containsInput(null, resource) + "}");
                    queries.add(head + ",\"op\":\"type_based\",\"result\":" + quote(idOf(holders, cache.findTypeBasedRecipeHolder(null, resource))) + "}");
                    // O `ItemStack` também é `TypedInstance`: os componentes dele comparam como `PatchedDataComponentMap`, os do `ItemResource` sem patch não.
                    queries.add(head + ",\"op\":\"contains_stack\",\"result\":" + cache.containsInput(null, itemStack) + "}");
                    queries.add(head + ",\"op\":\"type_based_stack\",\"result\":" + quote(idOf(holders, cache.findTypeBasedRecipeHolder(null, itemStack))) + "}");
                }
            }
            lines.add("{\"case\":\"" + name + "\",\"kind\":\"" + SINGLE + "\",\"recipes\":" + join(recipeJson) + ",\"queries\":" + join(queries) + "}");
        }

        private static <R extends MekanismRecipe<?, ?, ?>> String idOf(List<RecipeHolder<R>> holders, RecipeHolder<R> found) {
            if (found == null) {
                return null;
            }
            for (int i = 0; i < holders.size(); i++) {
                if (holders.get(i) == found) {
                    return holders.get(i).id().identifier().getPath();
                }
            }
            throw new IllegalStateException("a receita achada não está na lista");
        }

        private static void doubles(String name, List<DoubleRecipe> recipes, List<Stack> items, List<Stack> chemicals) {
            List<RecipeHolder<ItemStackChemicalToItemStackRecipe<MetallurgicInfusingBookCategory, MetallurgicInfusingBookInfo>>> holders = new ArrayList<>();
            List<String> recipeJson = new ArrayList<>();
            for (DoubleRecipe recipe : recipes) {
                ItemStackIngredient item = IngredientCreatorAccess.item().from(recipe.item.ingredient(), recipe.count);
                ChemicalStackIngredient chemical = IngredientCreatorAccess.chemicalStack().fromHolder(chemicalHolder(recipe.chemical), recipe.amount);
                holders.add(new RecipeHolder<>(ResourceKey.create(Registries.RECIPE, Identifier.fromNamespaceAndPath("trace", recipe.id)),
                      new BasicMetallurgicInfuserRecipe(common(), new MetallurgicInfusingBookInfo(MetallurgicInfusingBookCategory.MISC, ""), item, chemical, false, 0F,
                            new ItemStackTemplate(Items.STONE))));
                recipeJson.add("{\"id\":\"" + recipe.id + "\",\"item\":" + recipe.item.json() + ",\"count\":" + recipe.count + ",\"chemical\":\"" + recipe.chemical
                      + "\",\"amount\":" + recipe.amount + "}");
            }
            InputRecipeCache.ItemChemical<ItemStackChemicalToItemStackRecipe<MetallurgicInfusingBookCategory, MetallurgicInfusingBookInfo>> cache = new InputRecipeCache.ItemChemical<>(
                  MekanismRecipeType.METALLURGIC_INFUSING.get(), ItemStackChemicalToItemStackRecipe::getItemInput, ItemStackChemicalToItemStackRecipe::getChemicalInput) {
                {
                    initCache(holders);
                    initialized = true;
                }
            };
            List<String> queries = new ArrayList<>();
            for (Stack item : items) {
                ItemStack itemStack = item.item();
                ItemResource itemResource = ItemResource.of(itemStack);
                if (!itemStack.isEmpty()) {
                    queries.add("{\"op\":\"contains_a\",\"a\":" + item.json() + ",\"result\":" + cache.containsInputA(null, itemResource) + "}");
                    queries.add("{\"op\":\"contains_a_stack\",\"a\":" + item.json() + ",\"result\":" + cache.containsInputA(null, itemStack) + "}");
                }
                for (Stack chemical : chemicals) {
                    ChemicalStack chemicalStack = chemical.id.isEmpty() ? ChemicalStack.EMPTY : new ChemicalStack(chemicalHolder(chemical.id), chemical.amount);
                    ChemicalResource chemicalResource = chemicalStack.isEmpty() ? ChemicalResource.EMPTY : ChemicalResource.of(chemicalStack.typeHolder());
                    String head = "{\"a\":" + item.json() + ",\"b\":" + chemical.json();
                    queries.add(head + ",\"op\":\"contains_ab\",\"result\":" + cache.containsInputAB(null, itemResource, chemicalResource) + "}");
                    queries.add(head + ",\"op\":\"contains_ba\",\"result\":" + cache.containsInputBA(null, itemResource, chemicalResource) + "}");
                    queries.add(head + ",\"op\":\"contains_ab_stack\",\"result\":" + cache.containsInputAB(null, itemStack, chemicalStack) + "}");
                    queries.add(head + ",\"op\":\"contains_ba_stack\",\"result\":" + cache.containsInputBA(null, itemStack, chemicalStack) + "}");
                    queries.add(head + ",\"op\":\"type_based_stack\",\"result\":" + quote(idOf(holders, cache.findTypeBasedRecipeHolder(null, itemStack, chemicalStack, 0,
                          (recipe, a, b, data) -> true))) + "}");
                    queries.add(head + ",\"op\":\"find_first\",\"result\":" + quote(idOf(holders, cache.findFirstRecipe(null, itemStack, chemicalStack))) + "}");
                    queries.add(head + ",\"op\":\"find_first_b\",\"result\":" + quote(idOf(holders, cache.findFirstRecipe(null, itemStack, chemicalStack, false))) + "}");
                    queries.add(head + ",\"op\":\"type_based\",\"result\":" + quote(idOf(holders, cache.findTypeBasedRecipeHolder(null, itemResource, chemicalResource, 0,
                          (recipe, a, b, data) -> true))) + "}");
                }
            }
            for (Stack chemical : chemicals) {
                if (!chemical.id.isEmpty()) {
                    ChemicalResource chemicalResource = ChemicalResource.of(chemicalHolder(chemical.id));
                    queries.add("{\"op\":\"contains_b\",\"b\":" + chemical.json() + ",\"result\":" + cache.containsInputB(null, chemicalResource) + "}");
                }
            }
            lines.add("{\"case\":\"" + name + "\",\"kind\":\"" + DOUBLE + "\",\"recipes\":" + join(recipeJson) + ",\"queries\":" + join(queries) + "}");
        }

        private static RegistryAccess registry;

        private static Holder<Chemical> chemicalHolder(String path) {
            ResourceKey<Chemical> key = switch (path) {
                case "redstone" -> ChemicalIds.REDSTONE;
                case "carbon" -> ChemicalIds.CARBON;
                case "diamond" -> ChemicalIds.DIAMOND;
                default -> throw new IllegalArgumentException(path);
            };
            return registry.getOrThrow(key);
        }
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
        MekanismTileContainer<TileEntityMekanism> menu;
        boolean recordUpgradeWindow;
        private List<ISyncableData> previousTracked = List.of();
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
        Player player() {
            if (player == null) {
                player = helper.makeMockPlayerLookingAt(pos, Direction.NORTH);
            }
            return player;
        }

        void openPlayer() {
            tile().open(player());
        }

        /// O contexto de um pacote: só o jogador, que é tudo o que os `handle` do Mekanism pedem.
        IPayloadContext context() {
            Player who = player();
            return (IPayloadContext) Proxy.newProxyInstance(IPayloadContext.class.getClassLoader(), new Class<?>[]{IPayloadContext.class},
                  (proxy, method, args) -> {
                      if (method.getName().equals("player")) {
                          return who;
                      }
                      throw new UnsupportedOperationException(method.getName());
                  });
        }

        /// Abre a janela como o servidor faz: cria o container, que põe o jogador em `playersUsing`, e o deixa como o menu aberto.
        void openMenu(ContainerTypeRegistryObject<?> type) {
            Player who = player();
            menu = new MekanismTileContainer<>(type, 100, who.getInventory(), (TileEntityMekanism) tile());
            who.containerMenu = menu;
        }

        void closeMenu() {
            menu.removed(player);
            player.containerMenu = player.inventoryMenu;
        }

        /// `PacketWindowSelect.handle`, o real.
        void selectWindow(SelectedWindowData window) {
            new PacketWindowSelect(window).handle(context());
        }

        /// `PacketGuiInteract.handle`, o real.
        void interact(GuiInteraction interaction) {
            new PacketGuiInteract(interaction, helper.absolutePos(pos), MekanismContainer.UPGRADE_WINDOW).handle(context());
        }

        /// `PacketRemoveUpgrade.handle`, o real.
        void removePacket(net.minecraft.resources.ResourceKey<Upgrade> key, boolean all) {
            new PacketRemoveUpgrade(helper.absolutePos(pos), upgrade(key), all).handle(context());
        }

        ItemStack upgradeStack(net.minecraft.resources.ResourceKey<Upgrade> key, int count) {
            return IUpgradeHelper.INSTANCE.asResource(upgrade(key)).toStack(count);
        }

        /// O primeiro upgrade do registro que a máquina não aceita.
        ItemStack unsupportedUpgradeStack() {
            TileComponentUpgrade component = tile().getComponent();
            Holder<Upgrade> holder = helper.getLevel().registryAccess().lookupOrThrow(MekanismRegistries.Keys.UPGRADES).listElements()
                  .filter(upgrade -> !component.supports(upgrade)).map(upgrade -> (Holder<Upgrade>) upgrade).findFirst().orElseThrow();
            mark("unsupported upgrade is " + holder.getRegisteredName());
            return IUpgradeHelper.INSTANCE.asResource(holder).toStack(1);
        }

        void setUpgradeOutput(net.minecraft.resources.ResourceKey<Upgrade> key, int count) {
            ItemResource resource = count == 0 ? ItemResource.EMPTY : IUpgradeHelper.INSTANCE.asResource(upgrade(key));
            machine().getComponent().getUpgradeOutputSlot().setContents(resource, count, null);
        }

        /// O jogador com o upgrade na mão clica no slot virtual: o caminho do `AbstractContainerMenu.clicked`.
        void clickUpgradeSlot(net.minecraft.resources.ResourceKey<Upgrade> key, int count) {
            menu.setCarried(upgradeStack(key, count));
            menu.clicked(menu.getUpgradeSlot().index, 0, ContainerInput.PICKUP, player);
        }

        void dropCarried() {
            menu.setCarried(ItemStack.EMPTY);
        }

        int playerSlotIndex(int inventorySlot) {
            for (Slot slot : menu.slots) {
                if (slot.container == player.getInventory() && slot.getContainerSlot() == inventorySlot) {
                    return slot.index;
                }
            }
            throw new IllegalStateException("no container slot for inventory slot " + inventorySlot);
        }

        private String window() {
            SelectedWindowData window = menu.getSelectedWindow(player.getUUID());
            return window == null ? "none" : window.type().name().toLowerCase();
        }

        private static String itemJson(ItemStack stack) {
            return "[\"" + (stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath() + (stack.has(IUpgradeHelper.INSTANCE.dataComponent())
                  ? ":" + stack.get(IUpgradeHelper.INSTANCE.dataComponent()).getRegisteredName().substring("mekanism:".length()) : "")) + "\"," + stack.getCount() + "]";
        }

        private String slotsJson() {
            StringBuilder out = new StringBuilder("[");
            for (Slot slot : menu.slots) {
                if (slot.hasItem()) {
                    out.append(out.length() == 1 ? "" : ",").append('[').append(slot.index).append(",\"").append(slot.getClass().getSimpleName()).append("\",")
                          .append(itemJson(slot.getItem()).substring(1));
                }
            }
            return out.append(']').toString();
        }

        private String marksJson() {
            StringBuilder out = new StringBuilder("[");
            for (int i = 0; i < marks.size(); i++) {
                out.append(i == 0 ? "" : ",").append('"').append(marks.get(i)).append('"');
            }
            marks.clear();
            return out.append(']').toString();
        }

        /// Todos os slots do container, na ordem, com a classe: onde cada coisa pode ir.
        void layout() {
            StringBuilder out = new StringBuilder("{\"t\":").append(tick++).append(",\"marks\":").append(marksJson()).append(",\"layout\":[");
            for (Slot slot : menu.slots) {
                out.append(slot.index == 0 ? "" : ",").append('[').append(slot.index).append(",\"").append(slot.getClass().getSimpleName()).append("\"]");
            }
            lines.add(out.append("],\"slots\":").append(slotsJson()).append('}').toString());
        }

        void snapshot() {
            lines.add("{\"t\":" + tick++ + ",\"marks\":" + marksJson() + ",\"window\":\"" + window() + "\",\"slots\":" + slotsJson()
                  + ",\"upgrade_ticks\":" + upgradeTicks(tile().getComponent()) + "}");
        }

        /// `quickMoveStack` a partir de um slot do inventário do jogador, que antes recebe o item.
        void quickMoveFromPlayer(int inventorySlot, ItemStack stack) {
            player.getInventory().setItem(inventorySlot, stack);
            quickMove(playerSlotIndex(inventorySlot));
        }

        void quickMoveFromMenu(int index) {
            quickMove(index);
        }

        private void quickMove(int index) {
            ItemStack returned = menu.quickMoveStack(player, index);
            lines.add("{\"t\":" + tick++ + ",\"marks\":" + marksJson() + ",\"from\":" + index + ",\"window\":\"" + window() + "\",\"returned\":"
                  + itemJson(returned) + ",\"slots\":" + slotsJson() + "}");
        }

        @SuppressWarnings("unchecked")
        private List<ISyncableData> trackedData() {
            try {
                Field field = MekanismContainer.class.getDeclaredField("trackedData");
                field.setAccessible(true);
                return (List<ISyncableData>) field.get(menu);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private static int upgradeTicks(TileComponentUpgrade component) {
            try {
                Field field = TileComponentUpgrade.class.getDeclaredField("upgradeTicks");
                field.setAccessible(true);
                return field.getInt(component);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private static String className(Object data) {
            return data.getClass().getName().substring(data.getClass().getName().lastIndexOf('.') + 1);
        }

        /// A ordem de `trackedData` com a classe de cada um, e o que entrou e saiu desde a linha anterior.
        void trackingLine() {
            List<ISyncableData> tracked = trackedData();
            StringBuilder out = new StringBuilder("{\"t\":").append(tick++).append(",\"marks\":").append(marksJson())
                  .append(",\"window\":\"").append(window()).append("\",\"players\":").append(tile().playersUsing.size())
                  .append(",\"count\":").append(tracked.size()).append(",\"added\":[");
            boolean first = true;
            for (int i = 0; i < tracked.size(); i++) {
                ISyncableData data = tracked.get(i);
                if (previousTracked.stream().noneMatch(old -> old == data)) {
                    out.append(first ? "" : ",").append('[').append(i).append(",\"").append(className(data)).append("\"]");
                    first = false;
                }
            }
            out.append("],\"removed\":[");
            first = true;
            for (int i = 0; i < previousTracked.size(); i++) {
                ISyncableData old = previousTracked.get(i);
                if (tracked.stream().noneMatch(data -> data == old)) {
                    out.append(first ? "" : ",").append('[').append(i).append(",\"").append(className(old)).append("\"]");
                    first = false;
                }
            }
            out.append("],\"tracked\":[");
            for (int i = 0; i < tracked.size(); i++) {
                out.append(i == 0 ? "" : ",").append('"').append(className(tracked.get(i))).append('"');
            }
            lines.add(out.append("]}").toString());
            previousTracked = new ArrayList<>(tracked);
        }


        void closePlayer() {
            tile().close(player);
        }

        void setTank(long amount) {
            IChemicalTank tank = ((TileEntityMetallurgicInfuser) machine()).infusionTank;
            Holder<Chemical> redstone = helper.getLevel().registryAccess().getOrThrow(ChemicalIds.REDSTONE);
            tank.setContents(amount == 0 ? ChemicalResource.EMPTY : ChemicalResource.of(redstone), amount, null);
        }

        void setOsmium(long amount) {
            IChemicalTank tank = ((TileEntityAdvancedElectricMachine<?, ?>) machine()).chemicalTank;
            Holder<Chemical> osmium = helper.getLevel().registryAccess().getOrThrow(ChemicalIds.OSMIUM);
            tank.setContents(amount == 0 ? ChemicalResource.EMPTY : ChemicalResource.of(osmium), amount, null);
        }

        /// O `usedSoFar` do `TileEntityAdvancedElectricMachine`, que é privado.
        private static int usedSoFar(TileEntityAdvancedElectricMachine<?, ?> machine) {
            try {
                Field field = TileEntityAdvancedElectricMachine.class.getDeclaredField("usedSoFar");
                field.setAccessible(true);
                return field.getInt(machine);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
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
            if (recordUpgradeWindow) {
                TileComponentUpgrade component = tile().getComponent();
                List<String> installed = new ArrayList<>();
                component.getInstalledTypes().stream().sorted(Comparator.comparing(Holder::getRegisteredName))
                      .forEach(type -> installed.add("\"" + type.getRegisteredName().substring("mekanism:".length()) + "\":" + component.getUpgrades(type)));
                line.append(",\"upgrade_ticks\":").append(upgradeTicks(component)).append(",\"upgrade_map\":{").append(String.join(",", installed)).append('}');
                if (menu != null) {
                    line.append(",\"window\":\"").append(window()).append("\",\"tracked\":").append(trackedData().size())
                          .append(",\"carried\":").append(itemJson(menu.getCarried())).append(",\"players\":").append(tile().playersUsing.size());
                }
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
            if (chamber instanceof TileEntityAdvancedElectricMachine<?, ?> advanced) {
                line.append(",\"tank\":").append(advanced.chemicalTank.amountAsLong());
                line.append(",\"gas_slot\":[");
                slot(line, chamber.getInventorySlots().get(1));
                line.append("],\"used_so_far\":").append(usedSoFar(advanced));
                line.append(",\"max_operations\":").append(chamber.getOperationsPerTick());
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
