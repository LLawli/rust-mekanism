package mekanism.common.tests.network;

import com.google.gson.JsonObject;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.common.content.network.FluidNetwork;
import mekanism.common.content.network.transmitter.MechanicalPipe;
import mekanism.common.content.network.transmitter.Transmitter;
import mekanism.common.lib.transmitter.ConnectionType;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.transmitter.TileEntityTransmitter;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.LayeredCauldronBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.GameTestPlayer;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Frente `troca-vizinho` (2026-11-14): o que o transmissor faz quando o bloco vizinho é trocado por outro (`Level.setBlock` por cima do antigo). Para cada caso, o transmissor no centro e o vizinho a
/// leste: depois de `SETTLE` ticks o estado (`before`), a troca, e o estado `SETTLE` ticks depois (`after`). O estado é o que o original tem: o tipo cru e o visível do lado leste, `getAllCurrentConnections`,
/// se há aceitador ligado (`getConnectedAcceptor`), `canConnectToAcceptor`, e, nos canos, o fluido e a quantidade da rede. Grava `trace/transmitter_swap.jsonl`.
///
/// Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois (`oficial_gametest_rastro.sh`).
@ForEachTest(groups = "trace")
public class TransmitterSwapTraceTest {

    private static final int TIMEOUT = 300 * SharedConstants.TICKS_PER_SECOND;
    private static final String BATCH = "trace_transmitter_swap";
    private static final BlockPos T = new BlockPos(2, 2, 2);
    private static final Direction SIDE = Direction.EAST;
    private static final int SETTLE = 8;

    private static final String PIPE = "mekanism:ultimate_mechanical_pipe";
    private static final String CABLE = "mekanism:basic_universal_cable";

    /// `mode`: `normal` deixa o tipo como nasce, `pull` põe `PULL` no lado leste do cano.
    private record Case(String name, String transmitter, String mode, String before, String after) {
    }

    private static final List<Case> CASES = List.of(
          new Case("cauldron_water_to_lava", PIPE, "pull", "water_cauldron_3", "lava_cauldron"),
          new Case("cauldron_water_to_lava_normal", PIPE, "normal", "water_cauldron_3", "lava_cauldron"),
          new Case("cauldron_empty_to_water", PIPE, "normal", "cauldron", "water_cauldron_3"),
          new Case("cauldron_water_3_to_1", PIPE, "normal", "water_cauldron_3", "water_cauldron_1"),
          new Case("cauldron_water_to_empty", PIPE, "normal", "water_cauldron_3", "cauldron"),
          new Case("cauldron_water_to_chest", PIPE, "normal", "water_cauldron_3", "chest"),
          new Case("chest_to_cauldron_water", PIPE, "normal", "chest", "water_cauldron_3"),
          new Case("stone_to_cauldron_water", PIPE, "normal", "stone", "water_cauldron_3"),
          new Case("cauldron_water_to_stone", PIPE, "normal", "water_cauldron_3", "stone"),
          new Case("cauldron_water_to_air", PIPE, "normal", "water_cauldron_3", "air"),
          new Case("air_to_cauldron_water", PIPE, "normal", "air", "water_cauldron_3"),
          new Case("cauldron_water_to_cauldron_pull_chest", PIPE, "pull", "water_cauldron_3", "chest"),
          new Case("tank_basic_to_ultimate", PIPE, "normal", "basic_fluid_tank", "ultimate_fluid_tank"),
          new Case("cauldron_water_to_tank", PIPE, "normal", "water_cauldron_3", "basic_fluid_tank"),
          new Case("cube_basic_to_ultimate", CABLE, "normal", "basic_energy_cube", "ultimate_energy_cube"),
          new Case("stone_to_cube", CABLE, "normal", "stone", "basic_energy_cube"),
          new Case("cube_to_stone", CABLE, "normal", "basic_energy_cube", "stone"),
          new Case("cube_to_chest", CABLE, "normal", "basic_energy_cube", "chest"),
          new Case("chest_to_cube", CABLE, "normal", "chest", "basic_energy_cube"),
          new Case("cube_to_air", CABLE, "normal", "basic_energy_cube", "air"));

    private static Item item(String id) {
        return BuiltInRegistries.ITEM.getValue(Identifier.parse(id));
    }

    private static BlockState neighbor(String name) {
        return switch (name) {
            case "air" -> Blocks.AIR.defaultBlockState();
            case "stone" -> Blocks.STONE.defaultBlockState();
            case "chest" -> Blocks.CHEST.defaultBlockState();
            case "cauldron" -> Blocks.CAULDRON.defaultBlockState();
            case "lava_cauldron" -> Blocks.LAVA_CAULDRON.defaultBlockState();
            case "water_cauldron_1" -> Blocks.WATER_CAULDRON.defaultBlockState().setValue(LayeredCauldronBlock.LEVEL, 1);
            case "water_cauldron_3" -> Blocks.WATER_CAULDRON.defaultBlockState().setValue(LayeredCauldronBlock.LEVEL, 3);
            default -> BuiltInRegistries.BLOCK.getValue(Identifier.parse("mekanism:" + name)).defaultBlockState();
        };
    }

    private static final class Scene {

        final List<String> lines = new ArrayList<>();
        MekGameTestHelper helper;
        GameTestPlayer placer;

        void reset(MekGameTestHelper helper) {
            this.helper = helper;
            for (int x = 0; x <= 4; x++) {
                for (int y = 1; y <= 4; y++) {
                    for (int z = 0; z <= 4; z++) {
                        helper.setBlock(new BlockPos(x, y, z), Blocks.AIR.defaultBlockState());
                    }
                }
            }
            for (ItemEntity entity : helper.getLevel().getEntitiesOfClass(ItemEntity.class, new AABB(helper.absolutePos(T)).inflate(8))) {
                entity.discard();
            }
            if (placer == null) {
                placer = (GameTestPlayer) helper.makeMockPlayerLookingAt(T.below(), Direction.UP);
                placer.gameMode.changeGameModeForPlayer(GameType.CREATIVE);
            }
        }

        TileEntityTransmitter tile() {
            return helper.getLevel().getBlockEntity(helper.absolutePos(T)) instanceof TileEntityTransmitter tile ? tile : null;
        }

        /// O transmissor posto como um jogador o poria.
        void placeTransmitter(String id) {
            Vec3 away = Vec3.atBottomCenterOf(helper.absolutePos(new BlockPos(2, 1, -6)));
            placer.snapTo(away.x, away.y, away.z, 0, 0);
            placer.setShiftKeyDown(false);
            ItemStack stack = new ItemStack(item(id));
            placer.setItemInHand(InteractionHand.MAIN_HAND, stack);
            BlockPos at = helper.absolutePos(T);
            BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(at).relative(Direction.UP, 0.5), Direction.UP, at, false);
            stack.useOn(new net.minecraft.world.item.context.UseOnContext(helper.getLevel(), placer, InteractionHand.MAIN_HAND, stack, hit));
            if (helper.getLevel().getBlockState(at).isAir()) {
                Block block = BuiltInRegistries.BLOCK.getValue(Identifier.parse(id));
                helper.setBlock(T, block.defaultBlockState());
            }
        }

        void setNeighbor(String name) {
            helper.setBlock(T.relative(SIDE), neighbor(name));
        }

        void mode(String mode) {
            TileEntityTransmitter tile = tile();
            if (tile == null || !mode.equals("pull")) {
                return;
            }
            Transmitter<?, ?, ?> t = tile.getTransmitter();
            t.setConnectionTypeRaw(SIDE, ConnectionType.PULL);
            t.onModeChange(SIDE);
            t.refreshConnections();
            t.notifyTileChange();
        }

        JsonObject snap() {
            TileEntityTransmitter tile = tile();
            JsonObject o = new JsonObject();
            BlockState state = helper.getLevel().getBlockState(helper.absolutePos(T.relative(SIDE)));
            o.addProperty("neighbor", BuiltInRegistries.BLOCK.getKey(state.getBlock()).getPath());
            if (tile == null) {
                o.addProperty("present", false);
                return o;
            }
            Transmitter<?, ?, ?> t = tile.getTransmitter();
            o.addProperty("present", true);
            o.addProperty("raw", t.getConnectionTypeRaw(SIDE).getSerializedName());
            o.addProperty("type", t.getConnectionType(SIDE).getSerializedName());
            o.addProperty("all", t.getAllCurrentConnections() & 0xFF);
            o.addProperty("cur_t", t.currentTransmitterConnections & 0xFF);
            o.addProperty("cur_a", t.getAcceptorCache().currentAcceptorConnections & 0xFF);
            o.addProperty("can_connect", t.canConnect(SIDE));
            o.addProperty("can_connect_to_acceptor", t.canConnectToAcceptor(SIDE));
            o.addProperty("acceptor", t.getAcceptorCache().getConnectedAcceptor(SIDE) != null);
            o.addProperty("can_insert", tile.canInsert(SIDE));
            o.addProperty("can_extract", tile.canExtract(SIDE));
            if (t instanceof MechanicalPipe pipe && pipe.getTransmitterNetwork() instanceof FluidNetwork network) {
                o.addProperty("fluid", network.getContainer().isEmpty() ? "" : network.getContainer().resource().typeHolder().getRegisteredName());
                o.addProperty("amount", network.getContainer().amountAsLong());
            }
            return o;
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

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the connection of a transmitter before and after its neighbor block is swapped for another one.")
    public static void swap(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(5, 5, 5).fill(0, 0, 0, 4, 0, 4, Blocks.STONE.defaultBlockState()));
        Scene scene = new Scene();
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            for (Case c : CASES) {
                JsonObject[] before = new JsonObject[1];
                sequence = sequence
                      .thenExecute(() -> {
                          scene.reset(helper);
                          scene.setNeighbor(c.before());
                          scene.placeTransmitter(c.transmitter());
                          scene.mode(c.mode());
                      })
                      .thenExecuteAfter(SETTLE, () -> {
                          before[0] = scene.snap();
                          scene.setNeighbor(c.after());
                      })
                      .thenExecuteAfter(SETTLE, () -> {
                          JsonObject o = new JsonObject();
                          o.addProperty("case", c.name());
                          o.addProperty("transmitter", c.transmitter());
                          o.addProperty("mode", c.mode());
                          o.add("before", before[0]);
                          o.add("after", scene.snap());
                          scene.lines.add(o.toString());
                      })
                      .thenExecuteAfter(SETTLE, () -> {
                          // Mais um passo: o que o transmissor tem quando tudo assentou (um braço que volta no tick seguinte aparece aqui).
                          JsonObject o = new JsonObject();
                          o.addProperty("case", c.name() + "#settled");
                          o.add("after", scene.snap());
                          scene.lines.add(o.toString());
                      });
            }
            sequence.thenExecute(() -> write("transmitter_swap", scene.lines)).thenSucceed();
        });
    }
}
