package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.common.block.transmitter.BlockLargeTransmitter;
import mekanism.common.block.transmitter.BlockSmallTransmitter;
import mekanism.common.lib.transmitter.ConnectionType;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.util.EnumUtils;
import mekanism.common.util.VoxelShapeUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.Direction;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.BooleanOp;
import net.minecraft.world.phys.shapes.VoxelShape;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava `transmitter_shape_all.jsonl` (frente `cabo-forma`, Fase 4): a forma real de um transmissor (`BlockTransmitter.getRealShape`, que é a de seleção sem o Configurator, a de
/// colisão e a visual) para TODA combinação de tipos de conexão dos seis lados (4^6 = 4096 por tamanho, pequeno e grande), com a mesma conta do original: o centro e o braço de cada lado
/// que não é `NONE` (`getSideForType`, na ordem de `Direction`), juntados por `VoxelShapeUtils.batchCombine(center, OR, true, shapes)`. Por linha (`large` é 0 ou 1; o arquivo tem de caber no limite de 1 MiB do jj): a ocupação da forma numa grade de 16 por
/// 16 por 16 pixels (um hash FNV-1a de 64 bits dos 4096 bits, o `toAabbs` testado no centro de cada célula), o número de arestas do `forAllEdges` e o hash
/// FNV-1a de 64 bits dos 6 doubles de cada aresta (`Double.doubleToLongBits`, na ordem do jogo). As caixas e as arestas inteiras vêm só nas combinações `NORMAL` e `NONE`, para depurar.
@ForEachTest(groups = "trace")
public class TransmitterShapeAllTraceTest {

    private static final ConnectionType[] TYPES = {ConnectionType.NORMAL, ConnectionType.PUSH, ConnectionType.PULL, ConnectionType.NONE};
    private static final String LETTERS = "NPLX";
    private static final long FNV_OFFSET = 0xcbf29ce484222325L;
    private static final long FNV_PRIME = 0x100000001b3L;

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = 120 * SharedConstants.TICKS_PER_SECOND, batch = "trace_transmitter_shape_all")
    @TestHolder(description = "Records the real shape of a transmitter for every combination of connection types.")
    public static void shapes(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(0, 0, 0, 2, 0, 2, Blocks.STONE.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  List<String> lines = new ArrayList<>();
                  for (boolean large : new boolean[]{false, true}) {
                      for (int key = 0; key < 4096; key++) {
                          lines.add(line(large, key));
                      }
                  }
                  write("transmitter_shape_all", lines);
              })
              .thenSucceed());
    }

    private static VoxelShape realShape(boolean large, int key) {
        List<VoxelShape> shapes = new ArrayList<>(EnumUtils.DIRECTIONS.length);
        for (Direction side : EnumUtils.DIRECTIONS) {
            ConnectionType type = TYPES[(key >> (side.ordinal() * 2)) & 0b11];
            if (type != ConnectionType.NONE) {
                shapes.add(large ? BlockLargeTransmitter.getSideForType(type, side) : BlockSmallTransmitter.getSideForType(type, side));
            }
        }
        VoxelShape center = large ? BlockLargeTransmitter.CENTER : BlockSmallTransmitter.CENTER;
        //Same as BlockTransmitter#getRealShape: the center alone when there are no arms, the batch combine otherwise
        return shapes.isEmpty() ? center : VoxelShapeUtils.batchCombine(center, BooleanOp.OR, true, shapes);
    }

    private static String line(boolean large, int key) {
        VoxelShape shape = realShape(large, key);
        StringBuilder kinds = new StringBuilder();
        boolean onlyNormal = true;
        for (Direction side : EnumUtils.DIRECTIONS) {
            int index = (key >> (side.ordinal() * 2)) & 0b11;
            kinds.append(LETTERS.charAt(index));
            onlyNormal &= index == 0 || index == 3;
        }
        long occupancy = FNV_OFFSET;
        List<AABB> aabbs = shape.toAabbs();
        for (int cell = 0; cell < 4096; cell += 8) {
            int bits = 0;
            for (int bit = 0; bit < 8; bit++) {
                int index = cell + bit;
                double x = (index % 16 + 0.5) / 16;
                double y = (index / 16 % 16 + 0.5) / 16;
                double z = (index / 256 + 0.5) / 16;
                boolean inside = false;
                for (AABB box : aabbs) {
                    if (box.minX <= x && x <= box.maxX && box.minY <= y && y <= box.maxY && box.minZ <= z && z <= box.maxZ) {
                        inside = true;
                        break;
                    }
                }
                bits |= (inside ? 1 : 0) << bit;
            }
            occupancy = (occupancy ^ bits) * FNV_PRIME;
        }
        List<double[]> edges = new ArrayList<>();
        shape.forAllEdges((x1, y1, z1, x2, y2, z2) -> edges.add(new double[]{x1, y1, z1, x2, y2, z2}));
        long hash = FNV_OFFSET;
        for (double[] edge : edges) {
            for (double value : edge) {
                hash = (hash ^ Double.doubleToLongBits(value)) * FNV_PRIME;
            }
        }
        StringBuilder out = new StringBuilder();
        out.append("{\"large\":").append(large ? 1 : 0).append(",\"kinds\":\"").append(kinds).append("\",\"occupancy\":\"").append(Long.toHexString(occupancy))
              .append("\",\"edges\":").append(edges.size()).append(",\"edge_hash\":\"").append(Long.toHexString(hash)).append('"');
        if (onlyNormal) {
            out.append(",\"box_list\":[");
            for (int i = 0; i < aabbs.size(); i++) {
                AABB box = aabbs.get(i);
                out.append(i == 0 ? "" : ",").append('[').append(box.minX).append(',').append(box.minY).append(',').append(box.minZ).append(',').append(box.maxX).append(',').append(box.maxY)
                      .append(',').append(box.maxZ).append(']');
            }
            out.append("],\"edge_list\":[");
            for (int i = 0; i < edges.size(); i++) {
                double[] edge = edges.get(i);
                out.append(i == 0 ? "" : ",").append('[').append(edge[0]).append(',').append(edge[1]).append(',').append(edge[2]).append(',').append(edge[3]).append(',').append(edge[4])
                      .append(',').append(edge[5]).append(']');
            }
            out.append(']');
        }
        return out.append('}').toString();
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
