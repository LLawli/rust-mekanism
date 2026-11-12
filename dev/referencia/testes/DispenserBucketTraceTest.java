package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import mekanism.common.block.states.BlockStateHelper;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.TileEntityEnergyCube;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.Container;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.DispenserBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o que um dispenser com um balde faz contra um bloco da frente (`dispenser_bucket.jsonl`) e o que o item solto de um dispenser vira, tick a tick
/// (`dispenser_item_trace.jsonl`):
///
/// - baldes: o balde de lava, o de água e o vazio contra o Heat Generator, uma máquina, o cabo, o tanque de mecânica e um bloco comum (pedra), com o bloco vazio, com lava e com
///   água dentro (`fluid_logged`, posto por `placeLiquid`): o que sobrou no dispenser, o estado do bloco depois e os itens soltos.
/// - itens: o Energy Tablet, o graveto e o carvão do dispenser contra o Heat Generator, vazio e com lava, com a energia vazia e com 1.000, por cada uma de quatro faces, repetidos: cada
///   item solto tick a tick (posição, se foi removido e por quê).
///
/// Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace")
public class DispenserBucketTraceTest {

    private static final BlockPos MACHINE = new BlockPos(2, 2, 2);
    private static final int WAIT = 10;
    private static final int ITEM_TICKS = 14;
    private static final int REPEATS = 10;
    private static final int TIMEOUT = 600 * SharedConstants.TICKS_PER_SECOND;
    private static final String BATCH = "trace_dispenser_bucket";
    private static final Direction BUCKET_FACE = Direction.EAST;

    private static final String[] TARGETS = {"mekanismgenerators:heat_generator", "mekanism:basic_energy_cube", "mekanism:enrichment_chamber", "mekanism:basic_universal_cable",
                                             "mekanism:basic_mechanical_pipe", "minecraft:stone"};
    private static final String[] PRE = {"empty", "lava", "water"};
    private static final String[] BUCKETS = {"lava_bucket", "water_bucket", "bucket", "cod_bucket", "powder_snow_bucket"};

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(5, 5, 5).fill(0, 1, 0, 4, 1, 4, Blocks.STONE.defaultBlockState()));
    }

    private static Item item(String id) {
        int colon = id.indexOf(':');
        Identifier key = colon < 0 ? Identifier.withDefaultNamespace(id) : Identifier.fromNamespaceAndPath(id.substring(0, colon), id.substring(colon + 1));
        return BuiltInRegistries.ITEM.getValue(key);
    }

    private static Block block(String id) {
        return BuiltInRegistries.BLOCK.getValue(Identifier.parse(id));
    }

    private static String describe(BlockState state) {
        String text = state.toString();
        return text.startsWith("Block{") ? text.substring(6).replace("}", "") : text;
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

    /// A cena: a máquina em `MACHINE`, o dispenser ao lado dela apontando para ela e o bloco de redstone atrás do dispenser.
    private static final class Scene {
        final List<String> lines = new ArrayList<>();
        MekGameTestHelper helper;
        Direction face = BUCKET_FACE;
        String label = "";

        void reset(MekGameTestHelper helper) {
            this.helper = helper;
            for (int x = 0; x <= 4; x++) {
                for (int y = 2; y <= 3; y++) {
                    for (int z = 0; z <= 4; z++) {
                        helper.setBlock(new BlockPos(x, y, z), Blocks.AIR.defaultBlockState());
                    }
                }
            }
            for (ItemEntity entity : around(6)) {
                entity.discard();
            }
            for (net.minecraft.world.entity.LivingEntity entity : helper.getLevel().getEntitiesOfClass(net.minecraft.world.entity.LivingEntity.class,
                  new AABB(helper.absolutePos(MACHINE)).inflate(3), e -> !(e instanceof net.minecraft.world.entity.player.Player))) {
                entity.discard();
            }
        }

        List<ItemEntity> around(double inflate) {
            return helper.getLevel().getEntitiesOfClass(ItemEntity.class, new AABB(helper.absolutePos(MACHINE)).inflate(inflate));
        }

        BlockPos machinePos() {
            return helper.absolutePos(MACHINE);
        }

        /// O alvo posto como um jogador o poria (os blocos de mod, com a regra de colocação), ou o bloco do vanilla.
        void place(String id) {
            if (id.startsWith("minecraft:")) {
                helper.setBlock(MACHINE, block(id).defaultBlockState());
                return;
            }
            Player player = helper.makeMockPlayerLookingAt(2, 1, 2, Direction.UP);
            helper.useOn(new BlockPos(2, 1, 2), new ItemStack(item(id)), player, Direction.UP);
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

        void setEnergy(int energy) {
            if (energy < 0) {
                return;
            }
            if (helper.getLevel().getBlockEntity(machinePos()) instanceof TileEntityHeatGenerator generator) {
                generator.energyContainer().setEnergy(energy, null);
            } else if (helper.getLevel().getBlockEntity(machinePos()) instanceof TileEntityEnergyCube cube) {
                cube.energyContainer().setEnergy(energy, null);
            }
        }

        void arm(Direction facing, ItemStack stack) {
            face = facing;
            BlockPos pos = MACHINE.relative(facing);
            helper.setBlock(pos, Blocks.DISPENSER.defaultBlockState().setValue(DispenserBlock.FACING, facing.getOpposite()));
            ((Container) helper.getLevel().getBlockEntity(helper.absolutePos(pos))).setItem(0, stack);
            helper.setBlock(pos.relative(facing), Blocks.REDSTONE_BLOCK.defaultBlockState());
        }

        String contents() {
            Container container = (Container) helper.getLevel().getBlockEntity(helper.absolutePos(MACHINE.relative(face)));
            StringBuilder out = new StringBuilder("[");
            boolean first = true;
            for (int i = 0; i < container.getContainerSize(); i++) {
                ItemStack stack = container.getItem(i);
                if (!stack.isEmpty()) {
                    out.append(first ? "" : ",").append("[\"").append(BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath()).append("\",").append(stack.getCount()).append(']');
                    first = false;
                }
            }
            return out.append(']').toString();
        }

        String dropped() {
            Map<String, Integer> counts = new LinkedHashMap<>();
            for (ItemEntity entity : around(3)) {
                counts.merge(BuiltInRegistries.ITEM.getKey(entity.getItem().getItem()).getPath(), entity.getItem().getCount(), Integer::sum);
            }
            StringBuilder out = new StringBuilder("[");
            boolean first = true;
            for (Map.Entry<String, Integer> entry : counts.entrySet()) {
                out.append(first ? "" : ",").append("[\"").append(entry.getKey()).append("\",").append(entry.getValue()).append(']');
                first = false;
            }
            return out.append(']').toString();
        }

        void recordBucket(String target, String pre, String bucket) {
            BlockState state = helper.getLevel().getBlockState(machinePos());
            String logged = state.hasProperty(BlockStateHelper.FLUID_LOGGED) ? state.getValue(BlockStateHelper.FLUID_LOGGED).getSerializedName() : "-";
            String fluid = BuiltInRegistries.FLUID.getKey(state.getFluidState().getType()).getPath();
            lines.add("{\"target\":\"" + target + "\",\"pre\":\"" + pre + "\",\"item\":\"" + bucket + "\",\"dispenser\":" + contents() + ",\"block\":\"" + describe(state)
                  + "\",\"logged\":\"" + logged + "\",\"fluid\":\"" + fluid + "\",\"dropped\":" + dropped() + ",\"mobs\":" + mobs() + "}");
        }

        /// Os seres vivos soltos perto da máquina (o peixe de um balde de peixe, que `checkExtraContent` solta).
        int mobs() {
            return helper.getLevel().getEntitiesOfClass(net.minecraft.world.entity.LivingEntity.class, new AABB(helper.absolutePos(MACHINE)).inflate(3),
                  entity -> !(entity instanceof net.minecraft.world.entity.player.Player)).size();
        }
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records a dispenser with a bucket in front of a block.")
    public static void buckets(final DynamicTest test) {
        template(test);
        Scene scene = new Scene();
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            for (String target : TARGETS) {
                for (String pre : PRE) {
                    for (String bucket : BUCKETS) {
                        sequence.thenExecuteAfter(1, () -> {
                            scene.reset(helper);
                            scene.place(target);
                            scene.fillGenerator();
                            scene.logFluid(pre);
                            scene.arm(BUCKET_FACE, new ItemStack(item(bucket), bucket.equals("bucket") ? 2 : 1));
                        });
                        sequence.thenExecuteAfter(WAIT, () -> scene.recordBucket(target.substring(target.indexOf(':') + 1), pre, bucket));
                    }
                }
            }
            sequence.thenExecuteAfter(1, () -> write("dispenser_bucket", scene.lines));
            sequence.thenSucceed();
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the items a dispenser spawns in front of the Heat Generator, tick by tick.")
    public static void items(final DynamicTest test) {
        template(test);
        Scene scene = new Scene();
        Map<ItemEntity, Integer> seen = new IdentityHashMap<>();
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            for (Direction face : List.of(Direction.NORTH, Direction.UP, Direction.DOWN, Direction.WEST)) {
                for (String pre : List.of("empty", "lava")) {
                    for (String id : List.of("mekanism:energy_tablet", "stick", "coal")) {
                        for (int energy : List.of(1000)) {
                            for (int repeat = 0; repeat < REPEATS; repeat++) {
                                int[] tick = {0};
                                String name = face.getSerializedName() + "_" + pre + "_" + id.replace("mekanism:", "") + "@" + energy + "#" + repeat;
                                sequence.thenExecuteAfter(1, () -> {
                                    scene.reset(helper);
                                    scene.place("mekanismgenerators:heat_generator");
                                    scene.fillGenerator();
                                    scene.logFluid(pre);
                                    scene.setEnergy(energy);
                                    seen.clear();
                                    tick[0] = 0;
                                    scene.arm(face, new ItemStack(item(id), 3));
                                });
                                for (int t = 1; t <= ITEM_TICKS; t++) {
                                    sequence.thenExecuteAfter(1, () -> {
                                        tick[0]++;
                                        for (ItemEntity entity : scene.around(3)) {
                                            seen.putIfAbsent(entity, tick[0]);
                                        }
                                        if (tick[0] == ITEM_TICKS) {
                                            StringBuilder line = new StringBuilder("{\"case\":\"").append(name).append("\",\"items\":[");
                                            boolean first = true;
                                            for (Map.Entry<ItemEntity, Integer> entry : seen.entrySet()) {
                                                ItemEntity entity = entry.getKey();
                                                line.append(first ? "" : ",").append("{\"item\":\"").append(BuiltInRegistries.ITEM.getKey(entity.getItem().getItem()).getPath())
                                                      .append("\",\"first\":").append(entry.getValue()).append(",\"removed\":").append(entity.isRemoved()).append(",\"reason\":\"")
                                                      .append(entity.getRemovalReason()).append("\",\"pos\":[").append(rnd(entity.getX() - scene.machinePos().getX())).append(',')
                                                      .append(rnd(entity.getY() - scene.machinePos().getY())).append(',').append(rnd(entity.getZ() - scene.machinePos().getZ()))
                                                      .append("],\"lava\":").append(entity.isInLava()).append(",\"age\":").append(entity.tickCount).append('}');
                                                first = false;
                                            }
                                            scene.lines.add(line.append("],\"dispenser\":").append(scene.contents()).append(",\"dropped\":").append(scene.dropped()).append('}').toString());
                                        }
                                    });
                                }
                            }
                        }
                    }
                }
            }
            sequence.thenExecuteAfter(1, () -> write("dispenser_item_trace", scene.lines));
            sequence.thenSucceed();
        });
    }

    /// Um item parado na célula de um bloco com lava dentro (o Heat Generator e o cabo), tick a tick: `dispenser_lava_item.jsonl`. O item do dispenser atravessa a célula do gerador por dois ticks
    /// e sobrevive; o da `fluido_no_bloco_entity.jsonl` (pedra parada no cabo) morre no tick 2: aqui o mesmo item parado no gerador e no cabo, e o que o dispenser solta no cabo.
    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records an item inside a lava-logged cell.")
    public static void lavaItem(final DynamicTest test) {
        template(test);
        Scene scene = new Scene();
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            for (String target : List.of("mekanismgenerators:heat_generator", "mekanism:basic_universal_cable")) {
                for (String id : List.of("stone", "coal")) {
                    ItemEntity[] spawned = new ItemEntity[1];
                    String name = target.substring(target.indexOf(':') + 1) + "_" + id;
                    sequence.thenExecuteAfter(1, () -> {
                        scene.reset(helper);
                        scene.place(target);
                        scene.fillGenerator();
                        scene.logFluid("lava");
                        BlockPos pos = scene.machinePos();
                        spawned[0] = new ItemEntity(helper.getLevel(), pos.getX() + 0.5, pos.getY() + 0.05, pos.getZ() + 0.5, new ItemStack(item(id)));
                        spawned[0].setDeltaMovement(0, 0, 0);
                        helper.getLevel().addFreshEntity(spawned[0]);
                    });
                    for (int t = 1; t <= 8; t++) {
                        int tick = t;
                        sequence.thenExecuteAfter(1, () -> scene.lines.add("{\"case\":\"" + name + "\",\"t\":" + tick + ",\"removed\":" + spawned[0].isRemoved() + ",\"lava\":" + spawned[0].isInLava()
                              + ",\"y\":" + rnd(spawned[0].getY() - scene.machinePos().getY()) + ",\"fire\":" + spawned[0].getRemainingFireTicks() + "}"));
                    }
                }
            }
            sequence.thenExecuteAfter(1, () -> write("dispenser_lava_item", scene.lines));
            sequence.thenSucceed();
        });
    }
}
