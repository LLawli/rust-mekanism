package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.function.Supplier;
import mekanism.api.AutomationType;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.inventory.IInventorySlot;
import mekanism.common.recipe.MekanismRecipeType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.Container;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.crafting.RecipeMap;
import net.minecraft.world.item.crafting.RecipeType;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.DispenserBlock;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o que um dropper e um dispenser apontados para cada face das quatro máquinas portadas fazem (a máquina posta como um jogador a
/// põe, com o item e a configuração de lados padrão): o que entrou em cada slot, o que ficou no dropper e quantos itens soltos
/// apareceram. Um dropper com a capability de item do outro lado insere por ela (`DropperBlock.dispenseFrom` com o `Capabilities.Item.BLOCK`
/// do NeoForge); sem capability o item cai no chão como o `DefaultDispenseItemBehavior`. O dispenser nunca insere: solta o item.
@ForEachTest(groups = "trace")
public class DropperTraceTest {

    private static final BlockPos MACHINE = new BlockPos(2, 2, 2);
    private static final int WAIT = 10;
    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final List<Direction> FACES = List.of(Direction.DOWN, Direction.UP, Direction.NORTH, Direction.SOUTH, Direction.WEST, Direction.EAST);

    /// Os índices de `getInventorySlots()` de cada máquina: -1 onde ela não tem o slot.
    private record Spec(String name, Supplier<ItemLike> item, String valid, int infusion, int input, int output, int energy) {
    }

    private static final Spec CHAMBER = new Spec("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem(), "clay", -1, 0, 1, 2);
    private static final Spec CRUSHER = new Spec("crusher", () -> MekanismBlocks.CRUSHER.asItem(), "cobblestone", -1, 0, 1, 2);
    private static final Spec SMELTER = new Spec("energized_smelter", () -> MekanismBlocks.ENERGIZED_SMELTER.asItem(), "sand", -1, 0, 1, 2);
    private static final Spec INFUSER = new Spec("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.asItem(), "dirt", 0, 1, 2, 3);

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(5, 5, 5).fill(2, 1, 2, 2, 1, 2, Blocks.STONE.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_dropper")
    @TestHolder(description = "Records droppers and dispensers around the Enrichment Chamber.")
    public static void chamberDroppers(final DynamicTest test) {
        droppers(test, CHAMBER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_dropper")
    @TestHolder(description = "Records droppers and dispensers around the Crusher.")
    public static void crusherDroppers(final DynamicTest test) {
        droppers(test, CRUSHER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_dropper")
    @TestHolder(description = "Records droppers and dispensers around the Energized Smelter.")
    public static void smelterDroppers(final DynamicTest test) {
        droppers(test, SMELTER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_dropper")
    @TestHolder(description = "Records droppers and dispensers around the Metallurgic Infuser.")
    public static void infuserDroppers(final DynamicTest test) {
        droppers(test, INFUSER);
    }

    private static Item item(String id) {
        int colon = id.indexOf(':');
        Identifier key = colon < 0 ? Identifier.withDefaultNamespace(id) : Identifier.fromNamespaceAndPath(id.substring(0, colon), id.substring(colon + 1));
        return BuiltInRegistries.ITEM.getValue(key);
    }

    private static void droppers(DynamicTest test, Spec spec) {
        template(test);
        Scene scene = new Scene(spec);
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> scene.place(helper));
            String[] items = {"stick", spec.valid, "redstone", "mekanism:bio_fuel", "coal"};
            for (Block block : List.of(Blocks.DROPPER, Blocks.DISPENSER)) {
                for (Direction face : FACES) {
                    for (String id : items) {
                        sequence.thenExecuteAfter(1, () -> scene.arm(block, face, id));
                        sequence.thenExecuteAfter(WAIT, scene::record);
                    }
                }
            }
            sequence.thenExecuteAfter(1, () -> scene.finish());
            sequence.thenSucceed();
        });
    }

    private static final class Scene {
        private final Spec spec;
        private final List<String> lines = new ArrayList<>();
        private MekGameTestHelper helper;
        private String phase = "";
        private Direction at = Direction.UP;

        Scene(Spec spec) {
            this.spec = spec;
        }

        void place(MekGameTestHelper helper) {
            this.helper = helper;
            Player player = helper.makeMockPlayerLookingAt(2, 1, 2, Direction.UP);
            helper.useOn(new BlockPos(2, 1, 2), new ItemStack(spec.item.get()), player, Direction.UP);
            lines.add("{\"machine\":\"" + spec.name + "\",\"front\":\"" + tile().getDirection().getSerializedName() + "\"}");
        }

        TileEntityMekanism tile() {
            return (TileEntityMekanism) helper.getLevel().getBlockEntity(helper.absolutePos(MACHINE));
        }

        private IInventorySlot slot(int index) {
            return tile().getInventorySlots().get(index);
        }

        private void clearAll() {
            for (int i = 0; i < tile().getInventorySlots().size(); i++) {
                slot(i).setContents(ItemResource.EMPTY, 0, null);
            }
            if (spec.infusion >= 0) {
                ((TileEntityMetallurgicInfuser) tile()).infusionTank.setContents(ChemicalResource.EMPTY, 0, null);
            }
            for (Direction face : Direction.values()) {
                BlockPos pos = MACHINE.relative(face);
                helper.setBlock(pos, Blocks.AIR.defaultBlockState());
                helper.setBlock(pos.relative(face), Blocks.AIR.defaultBlockState());
            }
            for (ItemEntity entity : around()) {
                entity.discard();
            }
        }

        private List<ItemEntity> around() {
            return helper.getLevel().getEntitiesOfClass(ItemEntity.class, new AABB(helper.absolutePos(MACHINE)).inflate(3));
        }

        /// Um dropper ou dispenser em `face`, apontando para a máquina, com 3 itens, e um bloco de redstone atrás dele.
        void arm(Block block, Direction face, String id) {
            clearAll();
            at = face;
            phase = (block == Blocks.DROPPER ? "dropper_" : "dispenser_") + face.getSerializedName() + "_" + id.replace("mekanism:", "");
            BlockPos pos = MACHINE.relative(face);
            helper.setBlock(pos, block.defaultBlockState().setValue(DispenserBlock.FACING, face.getOpposite()));
            ((Container) helper.getLevel().getBlockEntity(helper.absolutePos(pos))).setItem(0, new ItemStack(item(id), 3));
            helper.setBlock(pos.relative(face), Blocks.REDSTONE_BLOCK.defaultBlockState());
        }

        void record() {
            StringBuilder line = new StringBuilder("{\"machine\":\"").append(spec.name).append("\",\"phase\":\"").append(phase).append("\",\"dropper\":[");
            BlockPos pos = MACHINE.relative(at);
            Container container = (Container) helper.getLevel().getBlockEntity(helper.absolutePos(pos));
            boolean first = true;
            for (int i = 0; i < container.getContainerSize(); i++) {
                ItemStack stack = container.getItem(i);
                if (!stack.isEmpty()) {
                    line.append(first ? "" : ",").append("[\"").append(BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath()).append("\",").append(stack.getCount()).append(']');
                    first = false;
                }
            }
            line.append("],\"slots\":[");
            List<IInventorySlot> slots = tile().getInventorySlots();
            for (int i = 0; i < slots.size(); i++) {
                ItemStack stack = slots.get(i).asItemStack();
                line.append(i == 0 ? "" : ",").append("[\"").append(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath())
                      .append("\",").append(stack.getCount()).append(']');
            }
            line.append(']');
            if (spec.infusion >= 0) {
                var tank = ((TileEntityMetallurgicInfuser) tile()).infusionTank;
                String id = tank.resource().isEmpty() ? "" : tank.resource().typeHolder().getRegisteredName().replace("mekanism:", "");
                line.append(",\"tank\":[\"").append(id).append("\",").append(tank.amountAsLong()).append(']');
            }
            int dropped = 0;
            for (ItemEntity entity : around()) {
                dropped += entity.getItem().getCount();
            }
            lines.add(line.append(",\"dropped\":").append(dropped).append('}').toString());
        }

        void finish() {
            clearAll();
            write("dropper_" + spec.name, lines);
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
