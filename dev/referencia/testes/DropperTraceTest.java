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
import mekanism.common.capabilities.Capabilities;
import mekanism.common.tile.TileEntityEnergyCube;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.transfer.ResourceHandler;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
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

/// Grava o que um dropper e um dispenser apontados para cada face das quatro máquinas portadas, do cubo de energia (basic e creative, com o contêiner de energia vazio e
/// com 1.000) e do Heat Generator fazem (a máquina posta como um jogador a
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
    private enum Kind { MACHINE, CUBE, GENERATOR }

    private record Spec(String name, Supplier<ItemLike> item, String valid, int infusion, int input, int output, int energy, Kind kind, List<String> items) {
        Spec(String name, Supplier<ItemLike> item, String valid, int infusion, int input, int output, int energy) {
            this(name, item, valid, infusion, input, output, energy, Kind.MACHINE, List.of("stick", valid, "redstone", "mekanism:bio_fuel", "coal"));
        }

        /// O contêiner de energia vazio e com 1.000: só o cubo e o gerador têm regra de slot que olha a energia guardada.
        List<Integer> energies() {
            return kind == Kind.MACHINE ? List.of(-1) : List.of(0, 1000);
        }
    }

    private static final Spec CHAMBER = new Spec("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem(), "clay", -1, 0, 1, 2);
    private static final Spec CRUSHER = new Spec("crusher", () -> MekanismBlocks.CRUSHER.asItem(), "cobblestone", -1, 0, 1, 2);
    private static final Spec SMELTER = new Spec("energized_smelter", () -> MekanismBlocks.ENERGIZED_SMELTER.asItem(), "sand", -1, 0, 1, 2);
    private static final Spec INFUSER = new Spec("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.asItem(), "dirt", 0, 1, 2, 3);

    private static final Spec CUBE = new Spec("energy_cube_basic", () -> MekanismBlocks.BASIC_ENERGY_CUBE.asItem(), "mekanism:energy_tablet", -1, -1, -1, -1, Kind.CUBE,
          List.of("stick", "mekanism:energy_tablet", "redstone", "mekanism:bio_fuel", "coal"));
    private static final Spec CREATIVE_CUBE = new Spec("energy_cube_creative", () -> MekanismBlocks.CREATIVE_ENERGY_CUBE.asItem(), "mekanism:energy_tablet", -1, -1, -1, -1, Kind.CUBE,
          List.of("stick", "mekanism:energy_tablet", "redstone", "mekanism:bio_fuel", "coal"));
    private static final Spec HEAT = new Spec("heat_generator", () -> GeneratorsBlocks.HEAT_GENERATOR.asItem(), "lava_bucket", -1, -1, -1, -1, Kind.GENERATOR,
          List.of("stick", "lava_bucket", "mekanism:energy_tablet", "mekanism:bio_fuel", "coal"));
    /// O que sai por cada face com a capability de item (`extract` do `ResourceHandler`, que é o que um hopper, um ejetor ou um cano pede): um item sem uso, um que o slot de
    /// energia converte, um que ele carrega, o combustível, o balde de lava e o balde vazio.
    private static final List<String> EXTRACTED = List.of("cobblestone", "redstone", "mekanism:energy_tablet", "coal", "lava_bucket", "bucket");

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

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_dropper")
    @TestHolder(description = "Records droppers, dispensers and extraction around the basic Energy Cube.")
    public static void cubeDroppers(final DynamicTest test) {
        droppers(test, CUBE);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_dropper")
    @TestHolder(description = "Records droppers, dispensers and extraction around the creative Energy Cube.")
    public static void creativeCubeDroppers(final DynamicTest test) {
        droppers(test, CREATIVE_CUBE);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_dropper")
    @TestHolder(description = "Records droppers, dispensers and extraction around the Heat Generator.")
    public static void heatGeneratorDroppers(final DynamicTest test) {
        droppers(test, HEAT);
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
            for (Block block : List.of(Blocks.DROPPER, Blocks.DISPENSER)) {
                for (int energy : spec.energies()) {
                    for (Direction face : FACES) {
                        for (String id : spec.items) {
                            sequence.thenExecuteAfter(1, () -> scene.arm(block, face, id, energy));
                            sequence.thenExecuteAfter(WAIT, scene::record);
                        }
                    }
                }
            }
            if (spec.kind != Kind.MACHINE) {
                for (int energy : spec.energies()) {
                    for (Direction face : FACES) {
                        for (String id : EXTRACTED) {
                            sequence.thenExecuteAfter(1, () -> scene.extract(face, id, energy));
                        }
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
            if (spec.kind == Kind.GENERATOR) {
                // O tanque cheio: o combustível que o gerador aceita fica no slot em vez de queimar no tick seguinte.
                FluidResource lava = FluidResource.of(Fluids.LAVA);
                ((TileEntityHeatGenerator) tile()).lavaTank.setContents(lava, ((TileEntityHeatGenerator) tile()).lavaTank.capacityAsLong(lava), null);
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

        private void setEnergy(int energy) {
            if (energy < 0) {
                return;
            }
            if (spec.kind == Kind.CUBE) {
                ((TileEntityEnergyCube) tile()).energyContainer().setEnergy(energy, null);
            } else {
                ((TileEntityHeatGenerator) tile()).energyContainer().setEnergy(energy, null);
            }
        }

        private long energyNow() {
            return spec.kind == Kind.CUBE ? ((TileEntityEnergyCube) tile()).energyContainer().getAmountAsLong()
                                          : ((TileEntityHeatGenerator) tile()).energyContainer().getAmountAsLong();
        }

        /// O que `face` deixa sair de cada slot da máquina com `id` posto nele, na mesma hora (sem tick entre o `setContents` e o `extract`): a capability de item da face.
        void extract(Direction face, String id, int energy) {
            clearAll();
            setEnergy(energy);
            List<IInventorySlot> slots = tile().getInventorySlots();
            StringBuilder line = new StringBuilder("{\"machine\":\"").append(spec.name).append("\",\"phase\":\"extract_").append(face.getSerializedName()).append('_')
                  .append(id.replace("mekanism:", "")).append('@').append(energy).append("\",\"out\":[");
            for (int i = 0; i < slots.size(); i++) {
                clearSlots();
                slots.get(i).setContents(ItemResource.of(item(id)), 1, null);
                ResourceHandler<ItemResource> handler = helper.getCapability(Capabilities.ITEM.block(), MACHINE, face);
                int out = 0;
                if (handler != null) {
                    try (Transaction transaction = Transaction.openRoot()) {
                        out = handler.extract(ItemResource.of(item(id)), 1, transaction);
                    }
                }
                line.append(i == 0 ? "" : ",").append(handler == null ? -1 : out);
            }
            clearSlots();
            lines.add(line.append("]}").toString());
        }

        private void clearSlots() {
            for (int i = 0; i < tile().getInventorySlots().size(); i++) {
                slot(i).setContents(ItemResource.EMPTY, 0, null);
            }
        }

        private List<ItemEntity> around() {
            return helper.getLevel().getEntitiesOfClass(ItemEntity.class, new AABB(helper.absolutePos(MACHINE)).inflate(3));
        }

        /// Um dropper ou dispenser em `face`, apontando para a máquina, com 3 itens, e um bloco de redstone atrás dele.
        void arm(Block block, Direction face, String id, int energy) {
            clearAll();
            setEnergy(energy);
            at = face;
            phase = (block == Blocks.DROPPER ? "dropper_" : "dispenser_") + face.getSerializedName() + "_" + id.replace("mekanism:", "") + (energy < 0 ? "" : "@" + energy);
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
            if (spec.kind != Kind.MACHINE) {
                line.append(",\"energy\":").append(energyNow());
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
