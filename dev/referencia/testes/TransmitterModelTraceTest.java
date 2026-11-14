package mekanism.common.tests.network;

import com.mojang.math.Transformation;
import java.io.IOException;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.client.model.blockstate.TransmitterBlockStateModel;
import mekanism.client.model.data.TransmitterModelData;
import mekanism.client.model.data.TransmitterModelData.VisualConnectionStatus;
import mekanism.common.lib.transmitter.ConnectionType;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.util.EnumUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.Direction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;
import org.joml.Matrix4fc;
import org.joml.Vector3f;
import org.joml.Vector4f;

/// Rastro da frente `cabo-modelo` (2026-11-14): as partes do modelo do transmissor, para os testes em Rust (`mods/machinery_basis/core/tests/transmitter_model_reference.rs`) conferirem.
///
/// `transmitter_model_visual.jsonl`: para cada uma das 4096 combinações dos quatro `ConnectionType` nos seis lados (`EnumUtils.DIRECTIONS`), o `TransmitterModelData.setConnectionData` do original, no
/// transmissor comum e no `Diversion` (a conta de `getVisualStatus`): `c` são os `ConnectionType.ordinal()` dos seis lados e `v` os `VisualConnectionStatus.ordinal()` que o dado do modelo guarda.
/// `transmitter_model_parts.json`: o nome de cada `VisualConnectionStatus` (`partName()`), os 24 grupos de `ALL_PART_GROUPS`, e, para cada lado, a `Transformation` de `makeIconStatusTransform` (o
/// quaternion e a posição de pontos de amostra como o `FaceBakery.rotateVertexBy` os põe: `v - 0,5`, matriz, `+ 0,5`) e o `isDirectional` do `NoneSegmentRemap` para os nomes de textura do OBJ.
@ForEachTest(groups = "trace")
public class TransmitterModelTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;

    private static final float[][] POINTS = {
          {0, 0, 0}, {1, 0, 0}, {0, 1, 0}, {0, 0, 1}, {1, 1, 1}, {0.3125F, 0.6875F, 0.3125F}, {0.6875F, 0.3125F, 0.6875F}, {0.5F, 0.5F, 0.5F},
          {0.3125F, 0.3125F, 0.6875F}, {0.1F, 0.2F, 0.3F},
    };

    private static final String[] SLOTS = {
          "#center_down", "#center_up", "#center_north", "#center_south", "#center_east", "#center_west", "#side", "#side_opaque", "#center_opaque", "#side_glass", "#particle", "center_down",
    };

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_transmitter_model")
    @TestHolder(description = "Records the visual connection statuses TransmitterModelData derives for every connection combination and the contiguous-part transform of TransmitterBlockStateModel.")
    public static void transmitterModel(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(1, 1, 1));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(TransmitterModelTraceTest::write)
              .thenSucceed());
    }

    private static void write() {
        List<String> lines = new ArrayList<>();
        ConnectionType[] types = ConnectionType.values();
        int sides = EnumUtils.DIRECTIONS.length;
        int combinations = 1;
        for (int i = 0; i < sides; i++) {
            combinations *= types.length;
        }
        for (int diversion = 0; diversion < 2; diversion++) {
            for (int combination = 0; combination < combinations; combination++) {
                ConnectionType[] connections = new ConnectionType[sides];
                StringBuilder c = new StringBuilder();
                int rest = combination;
                for (int side = 0; side < sides; side++) {
                    connections[side] = types[rest % types.length];
                    c.append(connections[side].ordinal());
                    rest /= types.length;
                }
                TransmitterModelData data = diversion == 1 ? new TransmitterModelData.Diversion() : new TransmitterModelData();
                data.setConnectionData(connections);
                StringBuilder v = new StringBuilder();
                for (int side = 0; side < sides; side++) {
                    v.append(data.getConnectionType(side).ordinal());
                }
                lines.add("{\"d\":" + diversion + ",\"c\":\"" + c + "\",\"v\":\"" + v + "\"}");
            }
        }
        StringBuilder parts = new StringBuilder("{\"statuses\":[");
        boolean first = true;
        for (VisualConnectionStatus status : VisualConnectionStatus.VALUES) {
            parts.append(first ? "" : ",").append("{\"name\":\"").append(status.name()).append("\",\"part\":\"").append(status.partName()).append("\"}");
            first = false;
        }
        parts.append("],\"groups\":[");
        first = true;
        for (String group : new java.util.TreeSet<>(TransmitterBlockStateModel.ALL_PART_GROUPS)) {
            parts.append(first ? "" : ",").append('"').append(group).append('"');
            first = false;
        }
        parts.append("],\"contiguous_rotation\":").append(VisualConnectionStatus.CONTIGUOUS_ROTATION).append(",\"icon_transforms\":[");
        try {
            Method make = TransmitterBlockStateModel.Unbaked.class.getDeclaredMethod("makeIconStatusTransform", Direction.class);
            make.setAccessible(true);
            first = true;
            for (Direction direction : EnumUtils.DIRECTIONS) {
                Transformation transformation = (Transformation) make.invoke(null, direction);
                org.joml.Quaternionfc quaternion = transformation.leftRotation();
                Matrix4fc matrix = transformation.getMatrix();
                parts.append(first ? "" : ",").append("{\"side\":\"").append(direction.getSerializedName()).append("\",\"quaternion\":[")
                      .append(quaternion.x()).append(',').append(quaternion.y()).append(',').append(quaternion.z()).append(',').append(quaternion.w())
                      .append("],\"matrix\":").append(rows(matrix)).append(",\"points\":[");
                for (int i = 0; i < POINTS.length; i++) {
                    // `FaceBakery.rotateVertexBy(vertex, BLOCK_MIDDLE, matriz)`.
                    Vector3f vertex = new Vector3f(POINTS[i][0], POINTS[i][1], POINTS[i][2]);
                    Vector3f middle = new Vector3f(0.5F, 0.5F, 0.5F);
                    vertex.sub(middle);
                    matrix.transformPosition(vertex);
                    vertex.add(middle);
                    parts.append(i == 0 ? "" : ",").append('[').append(POINTS[i][0]).append(',').append(POINTS[i][1]).append(',').append(POINTS[i][2])
                          .append(',').append(vertex.x()).append(',').append(vertex.y()).append(',').append(vertex.z()).append(']');
                }
                parts.append("]}");
                first = false;
            }
            parts.append("],\"directional\":[");
            Class<?> remap = Class.forName("mekanism.client.model.blockstate.TransmitterBlockStateModel$NoneSegmentRemap");
            Method directional = remap.getDeclaredMethod("isDirectional", String.class);
            directional.setAccessible(true);
            first = true;
            for (String slot : SLOTS) {
                parts.append(first ? "" : ",").append("{\"slot\":\"").append(slot).append("\",\"directional\":").append((boolean) directional.invoke(null, slot)).append('}');
                first = false;
            }
            parts.append("]}");
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
        try {
            Path directory = Path.of("trace");
            Files.createDirectories(directory);
            Files.write(directory.resolve("transmitter_model_visual.jsonl"), lines);
            Files.writeString(directory.resolve("transmitter_model_parts.json"), parts.toString() + "\n");
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    /// As três linhas de cima da matriz, por linhas: `[[m00, m10, m20, m30], ...]` (o JOML guarda por coluna).
    private static String rows(Matrix4fc m) {
        Vector4f[] rows = {m.getRow(0, new Vector4f()), m.getRow(1, new Vector4f()), m.getRow(2, new Vector4f())};
        StringBuilder out = new StringBuilder("[");
        for (int r = 0; r < 3; r++) {
            out.append(r == 0 ? "" : ",").append('[').append(rows[r].x()).append(',').append(rows[r].y()).append(',').append(rows[r].z()).append(',').append(rows[r].w()).append(']');
        }
        return out.append(']').toString();
    }
}
