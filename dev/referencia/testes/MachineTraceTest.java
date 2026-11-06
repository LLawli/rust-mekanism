package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.api.MekanismRegistries;
import mekanism.api.RelativeSide;
import mekanism.api.inventory.IInventorySlot;
import mekanism.api.recipes.cache.CachedRecipe.OperationTracker.RecipeError;
import mekanism.api.upgrade.Upgrade;
import mekanism.api.upgrade.UpgradeIds;
import mekanism.common.block.attribute.Attribute;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismItems;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.tile.interfaces.IRedstoneControl.RedstoneControl;
import mekanism.common.tile.machine.TileEntityEnrichmentChamber;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.Container;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
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
                  trace.setEnergy(trace.chamber().energyContainer().getCapacityAsLong());
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("speed 8");
                  trace.addUpgrades(UpgradeIds.SPEED, 7);
                  trace.setEnergy(trace.chamber().energyContainer().getCapacityAsLong());
              })
              .thenExecuteAfter(20, () -> {
                  trace.mark("energy 8");
                  trace.addUpgrades(UpgradeIds.ENERGY, 8);
                  trace.setEnergy(trace.chamber().energyContainer().getCapacityAsLong());
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

    private static class Trace {

        private static final List<RecipeError> ERRORS = List.of(RecipeError.NOT_ENOUGH_ENERGY, RecipeError.NOT_ENOUGH_INPUT,
              RecipeError.NOT_ENOUGH_OUTPUT_SPACE, RecipeError.INPUT_DOESNT_PRODUCE_OUTPUT);
        private static final List<String> ERROR_NAMES = List.of("energy", "input", "output_space", "wrong_output");

        private final String name;
        private final List<String> lines = new ArrayList<>();
        private final List<String> marks = new ArrayList<>();
        private MekGameTestHelper helper;
        private int tick;
        boolean recordChest;

        Trace(String name) {
            this.name = name;
        }

        void start(MekGameTestHelper helper) {
            this.helper = helper;
        }

        void mark(String mark) {
            marks.add(mark);
        }

        TileEntityEnrichmentChamber chamber() {
            BlockEntity tile = helper.getLevel().getBlockEntity(helper.absolutePos(CHAMBER));
            return tile instanceof TileEntityEnrichmentChamber chamber ? chamber : null;
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

        void setSlot(int slot, Item item, int count) {
            chamber().getInventorySlots().get(slot).setContents(ItemResource.of(item), count, null);
        }

        void clearSlot(int slot) {
            chamber().getInventorySlots().get(slot).setContents(ItemResource.EMPTY, 0, null);
        }

        void setEnergy(long energy) {
            chamber().energyContainer().setEnergy(energy, null);
        }

        void addUpgrades(net.minecraft.resources.ResourceKey<Upgrade> key, int amount) {
            Holder<Upgrade> upgrade = helper.getLevel().registryAccess().lookupOrThrow(MekanismRegistries.Keys.UPGRADES).getOrThrow(key);
            chamber().getComponent().addUpgrades(helper.getLevel().registryAccess(), upgrade, amount);
        }

        private static void slot(StringBuilder line, IInventorySlot slot) {
            ItemStack stack = slot.asItemStack();
            line.append('"').append(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath())
                  .append("\",").append(stack.getCount());
        }

        void record() {
            if (helper == null) {
                return;
            }
            TileEntityEnrichmentChamber chamber = chamber();
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++).append(",\"marks\":[");
            for (int i = 0; i < marks.size(); i++) {
                line.append(i == 0 ? "" : ",").append('"').append(marks.get(i)).append('"');
            }
            marks.clear();
            line.append("],\"progress\":").append(chamber.getRecipeProgress())
                  .append(",\"required\":").append(chamber.getTicksRequired())
                  .append(",\"energy\":").append(chamber.energyContainer().getAmountAsLong())
                  .append(",\"capacity\":").append(chamber.energyContainer().getCapacityAsLong())
                  .append(",\"per_tick\":").append(chamber.energyContainer().getEnergyPerTick())
                  .append(",\"active\":").append(chamber.getActive())
                  .append(",\"block_active\":").append(Attribute.isActive(chamber.getBlockState()))
                  .append(",\"powered\":").append(chamber.isPowered())
                  .append(",\"comparator\":").append(chamber.getCurrentRedstoneLevel())
                  .append(",\"input\":[");
            slot(line, chamber.getInventorySlots().get(INPUT));
            line.append("],\"output\":[");
            slot(line, chamber.getInventorySlots().get(OUTPUT));
            line.append("],\"errors\":[");
            boolean first = true;
            for (int i = 0; i < ERRORS.size(); i++) {
                if (chamber.getWarningCheck(ERRORS.get(i)).getAsBoolean()) {
                    line.append(first ? "" : ",").append('"').append(ERROR_NAMES.get(i)).append('"');
                    first = false;
                }
            }
            line.append(']');
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
            lines.add(line.append('}').toString());
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
