package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.api.RelativeSide;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.TileEntityEnergyCube;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Frente `cubo-forma`: grava `BlockEnergyCube.getShape` (a seleção) e `getCollisionShape` do Energy Cube posto, com o tile, para cada um dos seis `facing`
/// e cada uma das 64 combinações de lados relativos habilitados (`slotInfo.isEnabled()`: o tipo de dado diferente de `NONE`), em `energy_cube_shapes.jsonl`.
/// Uma linha por caso: `{facing, mask, outline: [[x1,y1,z1,x2,y2,z2]...], collision: [...]}`, com `mask` o bit `RelativeSide.ordinal()` de cada lado habilitado.
@ForEachTest(groups = "trace")
public class EnergyCubeShapeTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final String COMMIT = "b2d04e166";
    private static final BlockPos P1 = new BlockPos(1, 1, 1);

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_energy_cube_shapes")
    @TestHolder(description = "The selection and collision shapes of a placed basic energy cube for every facing and every set of enabled sides.")
    public static void shapes(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(0, 0, 0, 2, 0, 2, Blocks.STONE.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> record(helper))
              .thenSucceed());
    }

    private static JsonArray boxes(net.minecraft.world.phys.shapes.VoxelShape shape) {
        JsonArray array = new JsonArray();
        for (AABB box : shape.toAabbs()) {
            JsonArray b = new JsonArray();
            b.add(box.minX);
            b.add(box.minY);
            b.add(box.minZ);
            b.add(box.maxX);
            b.add(box.maxY);
            b.add(box.maxZ);
            array.add(b);
        }
        return array;
    }

    private static void record(MekGameTestHelper helper) {
        List<String> lines = new ArrayList<>();
        JsonObject meta = new JsonObject();
        meta.addProperty("commit", COMMIT);
        meta.addProperty("file", "energy_cube_shapes.jsonl");
        lines.add(meta.toString());
        for (Direction facing : Direction.values()) {
            for (int mask = 0; mask < 64; mask++) {
                helper.setBlock(P1, Blocks.AIR.defaultBlockState());
                BlockState state = MekanismBlocks.BASIC_ENERGY_CUBE.value().defaultBlockState().setValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.FACING, facing);
                helper.setBlock(P1, state);
                TileEntityEnergyCube cube = (TileEntityEnergyCube) helper.getLevel().getBlockEntity(helper.absolutePos(P1));
                ConfigInfo info = cube.getConfig().getConfig(TransmissionType.ENERGY);
                for (RelativeSide side : RelativeSide.VALUES) {
                    info.setDataType((mask & (1 << side.ordinal())) != 0 ? DataType.INPUT : DataType.NONE, side);
                }
                BlockState placed = helper.getLevel().getBlockState(helper.absolutePos(P1));
                JsonObject line = new JsonObject();
                line.addProperty("facing", facing.getSerializedName());
                line.addProperty("mask", mask);
                line.add("outline", boxes(placed.getShape(helper.getLevel(), helper.absolutePos(P1))));
                line.add("collision", boxes(placed.getCollisionShape(helper.getLevel(), helper.absolutePos(P1), CollisionContext.empty())));
                lines.add(line.toString());
            }
        }
        try {
            Path file = Path.of("trace", "energy_cube_shapes.jsonl");
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }
}
