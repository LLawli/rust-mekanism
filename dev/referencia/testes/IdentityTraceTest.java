package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Supplier;
import mekanism.api.RelativeSide;
import mekanism.api.fluid.IFluidTank;
import mekanism.api.inventory.IInventorySlot;
import mekanism.common.inventory.container.tile.MekanismTileContainer;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.registration.impl.ContainerTypeRegistryObject;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismContainerTypes;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.prefab.TileEntityConfigurableMachine;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.registries.GeneratorsContainerTypes;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.world.Container;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.component.CustomData;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.ComposterBlock;
import net.minecraft.world.level.block.HopperBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o que o `ItemResource` faz com dois itens do mesmo id e componentes diferentes (nome e dados), nas quatro máquinas: por
/// hopper (de cima, tick a tick) e por shift-clique (`quickMoveStack`, nos dois sentidos). E o que o ejetor faz com uma
/// composteira ao lado, embaixo e em cima (`ComposterWrapper`). A máquina é posta como um jogador a poria, em (1, 1, 1).
@ForEachTest(groups = "trace")
public class IdentityTraceTest {

    private static final BlockPos MACHINE = new BlockPos(1, 1, 1);
    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final int HOPPER_TICKS = 40;

    private record Spec(String name, Supplier<ItemLike> item, Supplier<ContainerTypeRegistryObject<?>> menu, String valid, int input, int output) {
    }

    private static final Spec CHAMBER = new Spec("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem(),
          () -> MekanismContainerTypes.ENRICHMENT_CHAMBER, "clay", 0, 1);
    private static final Spec CRUSHER = new Spec("crusher", () -> MekanismBlocks.CRUSHER.asItem(), () -> MekanismContainerTypes.CRUSHER,
          "cobblestone", 0, 1);
    private static final Spec SMELTER = new Spec("energized_smelter", () -> MekanismBlocks.ENERGIZED_SMELTER.asItem(),
          () -> MekanismContainerTypes.ENERGIZED_SMELTER, "sand", 0, 1);
    private static final Spec INFUSER = new Spec("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.asItem(),
          () -> MekanismContainerTypes.METALLURGIC_INFUSER, "dirt", 1, 2);

    /// O Heat Generator: o slot de combustível é o 0, e o tanque de lava fica cheio para a máquina não queimar o que o hopper põe.
    private static final Spec HEAT = new Spec("heat_generator", () -> GeneratorsBlocks.HEAT_GENERATOR.asItem(),
          () -> GeneratorsContainerTypes.HEAT_GENERATOR, "coal", 0, -1);

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(1, 0, 1, 1, 0, 1, Blocks.STONE.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_identity")
    @TestHolder(description = "Records a hopper above the Enrichment Chamber and shift-clicks, with items of one id and different components.")
    public static void chamberIdentity(final DynamicTest test) {
        identity(test, CHAMBER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_identity")
    @TestHolder(description = "Records a hopper above the Crusher and shift-clicks, with items of one id and different components.")
    public static void crusherIdentity(final DynamicTest test) {
        identity(test, CRUSHER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_identity")
    @TestHolder(description = "Records a hopper above the Energized Smelter and shift-clicks, with items of one id and different components.")
    public static void smelterIdentity(final DynamicTest test) {
        identity(test, SMELTER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_identity")
    @TestHolder(description = "Records a hopper above the Metallurgic Infuser and shift-clicks, with items of one id and different components.")
    public static void infuserIdentity(final DynamicTest test) {
        identity(test, INFUSER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_identity")
    @TestHolder(description = "Records a hopper above the Heat Generator and shift-clicks, with fuel of one id and different components.")
    public static void heatGeneratorIdentity(final DynamicTest test) {
        identity(test, HEAT);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_identity")
    @TestHolder(description = "Records the Enrichment Chamber ejecting its output into a composter beside it, below it and above it.")
    public static void chamberComposter(final DynamicTest test) {
        template(test);
        Composter scene = new Composter(CHAMBER);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> scene.place(helper));
            // Nada de ejetar nos primeiros ticks: a máquina espera o primeiro `tickDelay`.
            sequence.thenExecuteAfter(2, () -> scene.begin("below_cake_level0", Direction.DOWN, 0, "cake", 3));
            sequence.thenExecuteAfter(60, () -> scene.begin("below_cake_level6", Direction.DOWN, 6, "cake", 3));
            sequence.thenExecuteAfter(70, () -> scene.begin("below_cake_level7", Direction.DOWN, 7, "cake", 2));
            sequence.thenExecuteAfter(40, () -> scene.begin("below_cake_level8", Direction.DOWN, 8, "cake", 2));
            sequence.thenExecuteAfter(40, () -> scene.begin("below_stick_level0", Direction.DOWN, 0, "stick", 2));
            sequence.thenExecuteAfter(40, () -> scene.begin("below_leaves_level0", Direction.DOWN, 0, "oak_leaves", 1));
            sequence.thenExecuteAfter(40, () -> scene.begin("above_cake_level0", Direction.UP, 0, "cake", 2));
            sequence.thenExecuteAfter(40, () -> scene.begin("east_cake_level0", Direction.EAST, 0, "cake", 2));
            sequence.thenExecuteAfter(40, () -> scene.stop());
            sequence.thenExecuteAfter(1, scene::finish);
            sequence.thenSucceed();
        });
    }

    private static Item item(String id) {
        int colon = id.indexOf(':');
        Identifier key = colon < 0 ? Identifier.withDefaultNamespace(id) : Identifier.fromNamespaceAndPath(id.substring(0, colon), id.substring(colon + 1));
        return BuiltInRegistries.ITEM.getValue(key);
    }

    /// Um item com nome e dados: `name` e `data` vazios deixam o componente de fora.
    private static ItemStack stack(String id, String name, String data, int count) {
        ItemStack stack = new ItemStack(item(id), count);
        if (!name.isEmpty()) {
            stack.set(DataComponents.CUSTOM_NAME, Component.literal(name));
        }
        if (!data.isEmpty()) {
            CompoundTag tag = new CompoundTag();
            tag.putInt("d", Integer.parseInt(data));
            stack.set(DataComponents.CUSTOM_DATA, CustomData.of(tag));
        }
        return stack;
    }

    /// `["id","nome","dados",quantidade]`; vazio é `["",  "", "", 0]`.
    private static String json(ItemStack stack) {
        if (stack.isEmpty()) {
            return "[\"\",\"\",\"\",0]";
        }
        Component name = stack.get(DataComponents.CUSTOM_NAME);
        CustomData data = stack.get(DataComponents.CUSTOM_DATA);
        return "[\"" + BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath() + "\",\"" + (name == null ? "" : name.getString()) + "\",\""
              + (data == null ? "" : data.copyTag().getIntOr("d", 0)) + "\"," + stack.getCount() + "]";
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

    private static BlockState hopperState(Direction pointing) {
        return Blocks.HOPPER.defaultBlockState().setValue(HopperBlock.FACING, pointing);
    }

    private static void identity(DynamicTest test, Spec spec) {
        template(test);
        Scene scene = new Scene(spec);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> scene.place(helper));
            // Slot com o item A; o hopper tem B, simples e A, nessa ordem: só A entra.
            sequence.thenExecuteAfter(2, () -> scene.hopper("name_a_in_slot", spec.valid, "A", "", 3,
                  List.of(stack(spec.valid, "B", "", 2), stack(spec.valid, "", "", 2), stack(spec.valid, "A", "", 2))));
            // Slot vazio: entra o primeiro, B, e depois só B.
            sequence.thenExecuteAfter(HOPPER_TICKS + 2, () -> scene.hopper("empty_slot", null, "", "", 0,
                  List.of(stack(spec.valid, "B", "", 2), stack(spec.valid, "A", "", 2))));
            // O item simples no slot: só o simples entra.
            sequence.thenExecuteAfter(HOPPER_TICKS + 2, () -> scene.hopper("plain_in_slot", spec.valid, "", "", 3,
                  List.of(stack(spec.valid, "A", "", 2), stack(spec.valid, "", "", 2))));
            // Os dados como componente.
            sequence.thenExecuteAfter(HOPPER_TICKS + 2, () -> scene.hopper("data_in_slot", spec.valid, "", "1", 3,
                  List.of(stack(spec.valid, "", "2", 2), stack(spec.valid, "", "1", 2))));
            // Slot cheio (63) com o mesmo item e o hopper com o igual: entra um e o resto fica.
            sequence.thenExecuteAfter(HOPPER_TICKS + 2, () -> scene.hopper("near_full", spec.valid, "A", "", 63,
                  List.of(stack(spec.valid, "A", "", 3))));
            sequence.thenExecuteAfter(HOPPER_TICKS + 2, () -> scene.stopHopper());
            // Shift-clique.
            sequence.thenExecuteAfter(2, () -> scene.openMenu());
            sequence.thenExecuteAfter(2, () -> scene.quickMove("slot_a_player_b", spec.valid, "A", "", 3, 9, stack(spec.valid, "B", "", 5)));
            sequence.thenExecuteAfter(2, () -> scene.quickMove("slot_a_player_a", spec.valid, "A", "", 3, 10, stack(spec.valid, "A", "", 5)));
            sequence.thenExecuteAfter(2, () -> scene.quickMove("slot_a_player_plain", spec.valid, "A", "", 3, 11, stack(spec.valid, "", "", 5)));
            sequence.thenExecuteAfter(2, () -> scene.quickMove("empty_slot_player_b", null, "", "", 0, 12, stack(spec.valid, "B", "", 5)));
            sequence.thenExecuteAfter(2, () -> scene.quickMove("slot_data_player_data2", spec.valid, "", "1", 3, 13, stack(spec.valid, "", "2", 5)));
            sequence.thenExecuteAfter(2, () -> scene.quickMove("slot_data_player_data1", spec.valid, "", "1", 3, 14, stack(spec.valid, "", "1", 5)));
            // Do slot para o jogador: a barra tem B e A, e o item A do slot soma ao A.
            sequence.thenExecuteAfter(2, () -> scene.quickMoveOut("slot_a_out_hotbar_b_a", spec.valid, "A", "", 8,
                  List.of(stack(spec.valid, "B", "", 10), stack(spec.valid, "A", "", 4))));
            sequence.thenExecuteAfter(2, () -> scene.quickMoveOut("slot_a_out_hotbar_b_only", spec.valid, "A", "", 8,
                  List.of(stack(spec.valid, "B", "", 64))));
            if (spec == HEAT) {
                // O que o slot de combustível e o de energia aceitam e recusam: o graveto queima (é combustível), a argila não, e o tablet só o de energia aceita.
                sequence.thenExecuteAfter(2, () -> scene.quickMove("stick_player", spec.valid, "A", "", 3, 9, stack("stick", "", "", 5)));
                sequence.thenExecuteAfter(2, () -> scene.quickMove("tablet_player", spec.valid, "A", "", 3, 10, stack("mekanism:energy_tablet", "", "", 1)));
                sequence.thenExecuteAfter(2, () -> scene.quickMove("tablet_player_fuel_empty", null, "", "", 0, 11, stack("mekanism:energy_tablet", "", "", 1)));
                sequence.thenExecuteAfter(2, () -> scene.quickMove("coal_player_fuel_empty", null, "", "", 0, 12, stack(spec.valid, "", "", 5)));
                sequence.thenExecuteAfter(2, () -> scene.quickMove("stick_player_fuel_empty", null, "", "", 0, 14, stack("stick", "", "", 5)));
                sequence.thenExecuteAfter(2, () -> scene.quickMove("lava_bucket_player", null, "", "", 0, 13, stack("lava_bucket", "", "", 1)));
                // Um item que não queima, com o slot de combustível vazio: prova que o slot recusa o que não é combustível.
                sequence.thenExecuteAfter(2, () -> scene.quickMove("clay_ball_player_fuel_empty", null, "", "", 0, 15, stack("clay_ball", "", "", 5)));
            }
            sequence.thenExecuteAfter(2, () -> scene.finish());
            sequence.thenSucceed();
        });
    }

    /// Uma máquina e o hopper de cima; o menu e o jogador do shift-clique.
    private static final class Scene {
        private final Spec spec;
        private final List<String> lines = new ArrayList<>();
        private MekGameTestHelper helper;
        private Player player;
        private MekanismTileContainer<TileEntityMekanism> menu;
        private String phase = "";
        private int phaseTick = -1;
        private boolean hopperOn;

        Scene(Spec spec) {
            this.spec = spec;
        }

        void place(MekGameTestHelper helper) {
            this.helper = helper;
            player = helper.makeMockPlayerLookingAt(1, 0, 1, Direction.UP);
            helper.useOn(new BlockPos(1, 0, 1), new ItemStack(spec.item.get()), player, Direction.UP);
            String extra = "";
            if (spec == HEAT) {
                IFluidTank lava = tile().getFluidTanks().get(0);
                long capacity = lava.capacityAsLong(FluidResource.of(Fluids.LAVA));
                lava.setContents(FluidResource.of(Fluids.LAVA), capacity, null);
                extra = ",\"lava_capacity\":" + capacity + ",\"lava\":" + lava.amountAsLong();
            }
            lines.add("{\"machine\":\"" + spec.name + "\"" + extra + "}");
        }

        TileEntityMekanism tile() {
            BlockEntity tile = helper.getLevel().getBlockEntity(helper.absolutePos(MACHINE));
            return (TileEntityMekanism) tile;
        }

        private IInventorySlot slot(int index) {
            return tile().getInventorySlots().get(index);
        }

        private void setInput(String id, String name, String data, int count) {
            ItemStack stack = id == null ? ItemStack.EMPTY : stack(id, name, data, count);
            slot(spec.input).setContents(stack.isEmpty() ? ItemResource.EMPTY : ItemResource.of(stack), stack.isEmpty() ? 0 : count, null);
        }

        private Container hopperContainer() {
            return (Container) helper.getLevel().getBlockEntity(helper.absolutePos(MACHINE.above()));
        }

        void hopper(String label, String id, String name, String data, int count, List<ItemStack> hopperItems) {
            stopHopper();
            setInput(id, name, data, count);
            helper.setBlock(MACHINE.above(), hopperState(Direction.DOWN));
            Container container = hopperContainer();
            for (int i = 0; i < hopperItems.size(); i++) {
                container.setItem(i, hopperItems.get(i));
            }
            phase = "hopper_" + label;
            phaseTick = 0;
            hopperOn = true;
        }

        void stopHopper() {
            hopperOn = false;
            phaseTick = -1;
            if (helper.getLevel().getBlockState(helper.absolutePos(MACHINE.above())).is(Blocks.HOPPER)) {
                hopperContainer().clearContent();
            }
            helper.setBlock(MACHINE.above(), Blocks.AIR.defaultBlockState());
        }

        private String slotsJson() {
            StringBuilder out = new StringBuilder("[");
            List<IInventorySlot> slots = tile().getInventorySlots();
            for (int i = 0; i < slots.size(); i++) {
                out.append(i == 0 ? "" : ",").append(json(slots.get(i).asItemStack()));
            }
            return out.append(']').toString();
        }

        /// O Heat Generator gasta lava a cada tick; com o tanque cheio no fim de cada tick ele nunca tem lugar para queimar o combustível.
        private void refillLava() {
            if (spec == HEAT && helper != null) {
                IFluidTank lava = tile().getFluidTanks().get(0);
                lava.setContents(FluidResource.of(Fluids.LAVA), lava.capacityAsLong(FluidResource.of(Fluids.LAVA)), null);
            }
        }

        void record() {
            refillLava();
            if (!hopperOn || phaseTick < 0 || phaseTick >= HOPPER_TICKS) {
                return;
            }
            StringBuilder line = new StringBuilder("{\"phase\":\"").append(phase).append("\",\"t\":").append(phaseTick++).append(",\"hopper\":[");
            Container container = hopperContainer();
            for (int i = 0; i < container.getContainerSize(); i++) {
                line.append(i == 0 ? "" : ",").append(json(container.getItem(i)));
            }
            lines.add(line.append("],\"slots\":").append(slotsJson()).append('}').toString());
        }

        void openMenu() {
            stopHopper();
            menu = new MekanismTileContainer<>(spec.menu.get(), 100, player.getInventory(), tile());
            player.containerMenu = menu;
            StringBuilder layout = new StringBuilder("{\"layout\":[");
            for (Slot slot : menu.slots) {
                layout.append(slot.index == 0 ? "" : ",").append('[').append(slot.index).append(",\"").append(slot.getClass().getSimpleName()).append("\"]");
            }
            lines.add(layout.append("]}").toString());
        }

        private int playerSlotIndex(int inventorySlot) {
            for (Slot slot : menu.slots) {
                if (slot.container == player.getInventory() && slot.getContainerSlot() == inventorySlot) {
                    return slot.index;
                }
            }
            throw new IllegalStateException("no container slot for inventory slot " + inventorySlot);
        }

        private String menuJson() {
            StringBuilder out = new StringBuilder("[");
            boolean first = true;
            for (Slot slot : menu.slots) {
                if (slot.hasItem()) {
                    out.append(first ? "" : ",").append('[').append(slot.index).append(",\"").append(slot.getClass().getSimpleName()).append("\",")
                          .append(json(slot.getItem())).append(']');
                    first = false;
                }
            }
            return out.append(']').toString();
        }

        private void clearPlayer() {
            for (int i = 0; i < player.getInventory().getContainerSize(); i++) {
                player.getInventory().setItem(i, ItemStack.EMPTY);
            }
        }

        void quickMove(String label, String id, String name, String data, int count, int inventorySlot, ItemStack moving) {
            clearPlayer();
            setInput(id, name, data, count);
            player.getInventory().setItem(inventorySlot, moving);
            ItemStack returned = menu.quickMoveStack(player, playerSlotIndex(inventorySlot));
            lines.add("{\"phase\":\"quick_" + label + "\",\"from\":\"player\",\"returned\":" + json(returned) + ",\"menu\":" + menuJson() + "}");
        }

        /// O número no menu do slot de entrada da máquina: o do primeiro `InventoryContainerSlot` que guarda o item.
        private int machineMenuIndex() {
            for (Slot slot : menu.slots) {
                if (slot.getClass().getSimpleName().equals("InventoryContainerSlot") && slot.hasItem()) {
                    return slot.index;
                }
            }
            throw new IllegalStateException("no machine slot with an item");
        }

        void quickMoveOut(String label, String id, String name, String data, int count, List<ItemStack> hotbar) {
            clearPlayer();
            setInput(id, name, data, count);
            for (int i = 0; i < hotbar.size(); i++) {
                player.getInventory().setItem(i, hotbar.get(i));
            }
            ItemStack returned = menu.quickMoveStack(player, machineMenuIndex());
            lines.add("{\"phase\":\"quick_" + label + "\",\"from\":\"machine\",\"returned\":" + json(returned) + ",\"menu\":" + menuJson() + "}");
        }

        void finish() {
            write("identity_" + spec.name, lines);
        }
    }

    /// A Enrichment Chamber ejetando a saída para uma composteira.
    private static final class Composter {
        private final Spec spec;
        private final List<String> lines = new ArrayList<>();
        private MekGameTestHelper helper;
        private Player player;
        private String phase = "";
        private int phaseTick = -1;
        private Direction side = Direction.DOWN;

        Composter(Spec spec) {
            this.spec = spec;
        }

        void place(MekGameTestHelper helper) {
            this.helper = helper;
            player = helper.makeMockPlayerLookingAt(1, 0, 1, Direction.UP);
            helper.useOn(new BlockPos(1, 0, 1), new ItemStack(spec.item.get()), player, Direction.UP);
            lines.add("{\"machine\":\"" + spec.name + "\"}");
        }

        TileEntityMekanism tile() {
            return (TileEntityMekanism) helper.getLevel().getBlockEntity(helper.absolutePos(MACHINE));
        }

        private IInventorySlot output() {
            return tile().getInventorySlots().get(spec.output);
        }

        /// A saída só no lado `side`, ejetando; a composteira naquele lado no nível `level`, e a saída com `count` do item.
        void begin(String label, Direction side, int level, String id, int count) {
            this.side = side;
            for (Direction around : Direction.values()) {
                BlockPos pos = MACHINE.relative(around);
                helper.setBlock(pos, around == Direction.DOWN ? Blocks.STONE.defaultBlockState() : Blocks.AIR.defaultBlockState());
            }
            helper.setBlock(MACHINE.relative(side), Blocks.COMPOSTER.defaultBlockState().setValue(ComposterBlock.LEVEL, level));
            TileEntityConfigurableMachine machine = (TileEntityConfigurableMachine) tile();
            ConfigInfo info = machine.getConfig().getConfig(TransmissionType.ITEM);
            for (RelativeSide relative : RelativeSide.values()) {
                DataType type = relative.getDirection(tile().getDirection()) == side ? DataType.OUTPUT : DataType.NONE;
                info.setDataType(type, relative);
                machine.getConfig().sideChanged(TransmissionType.ITEM, relative);
            }
            info.setEjecting(true);
            output().setContents(ItemResource.of(item(id)), count, null);
            phase = label;
            phaseTick = 0;
        }

        void stop() {
            phaseTick = -1;
        }

        private int level() {
            BlockState state = helper.getLevel().getBlockState(helper.absolutePos(MACHINE.relative(side)));
            return state.is(Blocks.COMPOSTER) ? state.getValue(ComposterBlock.LEVEL) : -1;
        }

        void record() {
            if (phaseTick < 0 || phaseTick >= 60) {
                return;
            }
            lines.add("{\"phase\":\"" + phase + "\",\"t\":" + phaseTick++ + ",\"composter\":" + level() + ",\"output\":" + json(output().asItemStack()) + "}");
        }

        void finish() {
            write("composter_" + spec.name, lines);
        }
    }
}
