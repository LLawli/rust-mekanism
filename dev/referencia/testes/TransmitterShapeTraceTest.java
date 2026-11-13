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
import net.minecraft.SharedConstants;
import net.minecraft.core.Direction;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.VoxelShape;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava `transmitter_shape_edges.jsonl` (frente `cliente-mira`, decisão 166): para o transmissor pequeno e o grande, cada tipo de conexão (`NORMAL`, `PUSH`, `PULL`) e cada lado, a
/// forma do braço (`getSideForType`) em caixas (`toAabbs`) e as arestas que o contorno do bloco desenha dela (`VoxelShape.forAllEdges`, na ordem do jogo), mais o centro de cada um.
/// É o que o contorno do Configurator num braço `PUSH` ou `PULL` (três caixas empilhadas) mostra: as arestas da união, com os degraus.
@ForEachTest(groups = "trace")
public class TransmitterShapeTraceTest {

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = 60 * SharedConstants.TICKS_PER_SECOND, batch = "trace_transmitter_shape")
    @TestHolder(description = "Records the boxes and the outline edges of every transmitter arm shape per connection type and side.")
    public static void shapes(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(0, 0, 0, 2, 0, 2, Blocks.STONE.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  List<String> lines = new ArrayList<>();
                  for (boolean large : new boolean[]{false, true}) {
                      lines.add(line(large, "CENTER", null, large ? BlockLargeTransmitter.CENTER : BlockSmallTransmitter.CENTER));
                      for (ConnectionType type : new ConnectionType[]{ConnectionType.NORMAL, ConnectionType.PUSH, ConnectionType.PULL}) {
                          for (Direction side : Direction.values()) {
                              VoxelShape shape = large ? BlockLargeTransmitter.getSideForType(type, side) : BlockSmallTransmitter.getSideForType(type, side);
                              lines.add(line(large, type.name(), side, shape));
                          }
                      }
                  }
                  write("transmitter_shape_edges", lines);
              })
              .thenSucceed());
    }

    private static String line(boolean large, String type, Direction side, VoxelShape shape) {
        List<String> boxes = new ArrayList<>();
        for (AABB box : shape.toAabbs()) {
            boxes.add("[" + box.minX + "," + box.minY + "," + box.minZ + "," + box.maxX + "," + box.maxY + "," + box.maxZ + "]");
        }
        List<String> edges = new ArrayList<>();
        shape.forAllEdges((x1, y1, z1, x2, y2, z2) -> edges.add("[" + x1 + "," + y1 + "," + z1 + "," + x2 + "," + y2 + "," + z2 + "]"));
        return "{\"large\":" + large + ",\"type\":\"" + type + "\",\"side\":" + (side == null ? "null" : "\"" + side.getName() + "\"") + ",\"boxes\":[" + String.join(",", boxes)
              + "],\"edges\":[" + String.join(",", edges) + "]}";
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
