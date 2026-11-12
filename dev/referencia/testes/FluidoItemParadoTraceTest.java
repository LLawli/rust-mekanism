package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Um item parado dentro da célula de um bloco com colisão (o Heat Generator) e de um sem (o cabo), com lava, com água e sem fluido, tick a tick: `fluido_item_parado.jsonl`.
///
/// O que se quer saber é por que o item parado na célula com lava do gerador não pega fogo (`dispenser_lava_item.jsonl`: vivo, `isInLava` verdadeiro, fogo 0) e o do cabo morre:
/// `Entity.isAffectedByBlocks` é `!isRemoved() && !noPhysics`, e o `ItemEntity.tick` põe `noPhysics` quando a caixa dele está dentro de uma colisão. Cada linha traz `np` (o `noPhysics`),
/// `lava`/`water` (`isInLava`, `isInWater`), `fire` (`getRemainingFireTicks`), a posição relativa à máquina e o fluido da célula. Os casos com `burning` põem o item já em chamas
/// (a água apaga por `WaterFluid.entityInside`, que também é um efeito de bloco). O caso do cacto é o mesmo teste do `noPhysics` com outro efeito de bloco (`CactusBlock.entityInside`).
///
/// Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace")
public class FluidoItemParadoTraceTest {

    private static final BlockPos MACHINE = new BlockPos(2, 2, 2);
    private static final int TICKS = 12;
    private static final int REPEATS = 3;
    private static final int TIMEOUT = 600 * SharedConstants.TICKS_PER_SECOND;
    private static final String BATCH = "trace_fluido_item_parado";

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(5, 5, 5).fill(0, 1, 0, 4, 1, 4, Blocks.STONE.defaultBlockState()));
    }

    private static Item item(String id) {
        return BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(id));
    }

    private static Block block(String id) {
        return BuiltInRegistries.BLOCK.getValue(Identifier.parse(id));
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

    private static double rnd(double value) {
        return Math.round(value * 1000.0) / 1000.0;
    }

    private static final class Scene {
        final List<String> lines = new ArrayList<>();
        MekGameTestHelper helper;

        void reset(MekGameTestHelper helper) {
            this.helper = helper;
            for (int x = 0; x <= 4; x++) {
                for (int y = 2; y <= 3; y++) {
                    for (int z = 0; z <= 4; z++) {
                        helper.setBlock(new BlockPos(x, y, z), Blocks.AIR.defaultBlockState());
                    }
                }
            }
            helper.setBlock(MACHINE.below(), Blocks.STONE.defaultBlockState());
            for (ItemEntity entity : helper.getLevel().getEntitiesOfClass(ItemEntity.class, new net.minecraft.world.phys.AABB(helper.absolutePos(MACHINE)).inflate(6))) {
                entity.discard();
            }
        }

        BlockPos machinePos() {
            return helper.absolutePos(MACHINE);
        }

        void place(String id) {
            if (id.startsWith("minecraft:")) {
                if (id.equals("minecraft:cactus")) {
                    helper.setBlock(MACHINE.below(), Blocks.SAND.defaultBlockState());
                }
                helper.setBlock(MACHINE, block(id).defaultBlockState());
                return;
            }
            Player player = helper.makeMockPlayerLookingAt(2, 1, 2, Direction.UP);
            helper.useOn(new BlockPos(2, 1, 2), new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.parse(id))), player, Direction.UP);
        }

        void logFluid(String pre) {
            if (pre.equals("empty")) {
                return;
            }
            BlockState state = helper.getLevel().getBlockState(machinePos());
            if (!(state.getBlock() instanceof mekanism.common.block.states.IStateFluidLoggable loggable)) {
                return;
            }
            Fluid fluid = pre.equals("water") ? Fluids.WATER : Fluids.LAVA;
            loggable.placeLiquid(helper.getLevel(), machinePos(), state, fluid.defaultFluidState());
        }

        void fillGenerator() {
            if (helper.getLevel().getBlockEntity(machinePos()) instanceof TileEntityHeatGenerator generator) {
                FluidResource lava = FluidResource.of(Fluids.LAVA);
                generator.lavaTank.setContents(lava, generator.lavaTank.capacityAsLong(lava), null);
            }
        }
    }

    /// (nome, alvo, fluido, item, fogo inicial)
    private record Case(String name, String target, String pre, String item, int fire) {
    }

    private static List<Case> cases() {
        List<Case> out = new ArrayList<>();
        String gen = "mekanismgenerators:heat_generator";
        String cable = "mekanism:basic_universal_cable";
        for (int repeat = 0; repeat < REPEATS; repeat++) {
            out.add(new Case("heat_generator_lava_stone#" + repeat, gen, "lava", "stone", 0));
            out.add(new Case("heat_generator_lava_coal#" + repeat, gen, "lava", "coal", 0));
        }
        out.add(new Case("cable_lava_stone", cable, "lava", "stone", 0));
        out.add(new Case("heat_generator_lava_burning", gen, "lava", "stone", 100));
        out.add(new Case("heat_generator_water_burning", gen, "water", "stone", 100));
        out.add(new Case("heat_generator_empty_burning", gen, "empty", "stone", 100));
        out.add(new Case("cable_lava_burning", cable, "lava", "stone", 100));
        out.add(new Case("cable_water_burning", cable, "water", "stone", 100));
        out.add(new Case("cable_empty_burning", cable, "empty", "stone", 100));
        out.add(new Case("cactus_stone", "minecraft:cactus", "empty", "stone", 0));
        return out;
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records an item at rest inside a cell with a fluid, tick by tick.")
    public static void parado(final DynamicTest test) {
        template(test);
        Scene scene = new Scene();
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            for (Case c : cases()) {
                ItemEntity[] spawned = new ItemEntity[1];
                sequence.thenExecuteAfter(1, () -> {
                    scene.reset(helper);
                    scene.place(c.target());
                    scene.fillGenerator();
                    scene.logFluid(c.pre());
                    BlockPos pos = scene.machinePos();
                    spawned[0] = new ItemEntity(helper.getLevel(), pos.getX() + 0.5, pos.getY() + 0.05, pos.getZ() + 0.5, new ItemStack(item(c.item())));
                    spawned[0].setDeltaMovement(0, 0, 0);
                    if (c.fire() > 0) {
                        spawned[0].setRemainingFireTicks(c.fire());
                    }
                    helper.getLevel().addFreshEntity(spawned[0]);
                });
                for (int t = 1; t <= TICKS; t++) {
                    int tick = t;
                    sequence.thenExecuteAfter(1, () -> {
                        ItemEntity e = spawned[0];
                        BlockPos pos = scene.machinePos();
                        String cell = BuiltInRegistries.FLUID.getKey(helper.getLevel().getFluidState(pos).getType()).getPath();
                        scene.lines.add("{\"case\":\"" + c.name() + "\",\"t\":" + tick + ",\"removed\":" + e.isRemoved() + ",\"np\":" + e.noPhysics + ",\"lava\":" + e.isInLava()
                              + ",\"water\":" + e.isInWater() + ",\"fire\":" + e.getRemainingFireTicks() + ",\"x\":" + rnd(e.getX() - pos.getX()) + ",\"y\":" + rnd(e.getY() - pos.getY())
                              + ",\"z\":" + rnd(e.getZ() - pos.getZ()) + ",\"cell\":\"" + cell + "\"}");
                    });
                }
            }
            sequence.thenExecuteAfter(1, () -> write("fluido_item_parado", scene.lines));
            sequence.thenSucceed();
        });
    }
}
