package mekanism.common.tests.network;

import com.google.gson.JsonParser;
import com.mojang.math.OctahedralGroup;
import com.mojang.math.Transformation;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.common.tests.helpers.MekGameTestHelper;
import net.minecraft.SharedConstants;
import net.minecraft.client.renderer.block.dispatch.BlockModelRotation;
import net.minecraft.client.renderer.block.dispatch.ModelState;
import net.minecraft.core.Direction;
import net.neoforged.neoforge.client.model.UnbakedElementsHelper;
import net.neoforged.neoforge.common.util.TransformationHelper;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;
import org.joml.Matrix4fc;
import org.joml.Vector3f;
import org.joml.Vector4f;

/// Rastro da frente `transform-modelo` (2026-11-09): o `transform` de modelo do NeoForge, na conta que o jogo faz, para o teste em Rust (`mods/machinery_basis/core/tests/model_transform.rs`) conferir.
///
/// `model_transform`: para cada JSON de `transform` e cada rotação de blockstate: a `Transformation` que o `TransformationHelper.Deserializer` devolve (12 floats por linha, a matriz afim),
/// o `ModelState` de `UnbakedElementsHelper.composeRootTransformIntoModelState(BlockModelRotation, transformação)`, a posição de pontos de amostra como o `FaceBakery.rotateVertexBy` os põe
/// (`v - 0,5`, matriz do estado, `+ 0,5`) e a direção de cada face por `Direction.rotate(matriz do estado, direção)`. Um JSON que o NeoForge recusa grava `"error": true`.
@ForEachTest(groups = "trace")
public class ModelTransformTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;

    private static final String[] CASES = {
          "\"identity\"",
          "{\"translation\":[0,1,0]}",
          "{\"translation\":[0.5,0,0.5]}",
          "{\"translation\":[0,0,0]}",
          "{\"rotation\":[0,90,0]}",
          "{\"rotation\":[15,30,45],\"translation\":[0.1,0.2,0.3],\"scale\":1.5}",
          "{\"rotation\":{\"y\":45},\"origin\":\"center\"}",
          "{\"rotation\":[{\"x\":90},{\"y\":30}],\"scale\":[1,2,0.5],\"origin\":[0.25,0,0.75]}",
          "{\"left_rotation\":[0.1,0.2,0.3,0.9],\"right_rotation\":{\"z\":-30},\"translation\":[1,0,-1],\"origin\":\"corner\"}",
          "{\"matrix\":[[1,0,0,0.5],[0,0,-1,0],[0,1,0,2]]}",
          "[[0,1,0,0],[-1,0,0,1],[0,0,1,0]]",
          "{\"post-rotation\":[0,0,90],\"scale\":2}",
          "{\"scale\":0.5,\"origin\":\"opposing-corner\"}",
          "{\"translation\":[-0.25,2,0.125],\"rotation\":[0,-90,0],\"origin\":\"center\"}",
          // O que o NeoForge recusa:
          "{\"translation\":[0,1]}",
          "\"foo\"",
          "{\"rotation\":[1,2,3],\"left_rotation\":[0,0,0]}",
          "{\"matrix\":[[1,0,0,0],[0,1,0,0],[0,0,1,0]],\"scale\":1}",
          "{\"origin\":\"middle\"}",
          "{\"shear\":[1,0,0]}",
    };

    private static final float[][] POINTS = {
          {0, 0, 0}, {1, 0, 0}, {0, 1, 0}, {0, 0, 1}, {1, 1, 1}, {0.1F, 0.2F, 0.3F}, {0.75F, 0.5F, 0.9F}, {0.5F, 0.5F, 0.5F},
          {-0.25F, 1.5F, 0.0625F}, {2.0F, -1.0F, 0.3333333F},
    };

    private static final OctahedralGroup[] PARENTS = {
          OctahedralGroup.IDENTITY, OctahedralGroup.BLOCK_ROT_Y_90, OctahedralGroup.BLOCK_ROT_Y_180, OctahedralGroup.BLOCK_ROT_Y_270,
          OctahedralGroup.BLOCK_ROT_X_90, OctahedralGroup.BLOCK_ROT_Z_270,
    };

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_model_transform")
    @TestHolder(description = "Records NeoForge's model root transform: the deserialized matrix, the composed model state, the baked vertices and the rotated directions.")
    public static void modelTransform(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(1, 1, 1));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(ModelTransformTraceTest::write)
              .thenSucceed());
    }

    private static void write() {
        List<String> lines = new ArrayList<>();
        for (String json : CASES) {
            Transformation root;
            try {
                root = new TransformationHelper.Deserializer().deserialize(JsonParser.parseString(json), Transformation.class, null);
            } catch (RuntimeException e) {
                lines.add("{\"json\":" + quote(json) + ",\"error\":true}");
                continue;
            }
            for (OctahedralGroup group : PARENTS) {
                BlockModelRotation parent = BlockModelRotation.get(group);
                ModelState state = UnbakedElementsHelper.composeRootTransformIntoModelState(parent, root);
                Matrix4fc matrix = state.transformation().getMatrix();
                StringBuilder line = new StringBuilder("{\"json\":").append(quote(json))
                      .append(",\"parent\":").append(rows(parent.transformation().getMatrix()))
                      .append(",\"root\":").append(rows(root.getMatrix()))
                      .append(",\"state\":").append(rows(matrix))
                      .append(",\"identity\":").append(root.isIdentity())
                      .append(",\"points\":[");
                for (int i = 0; i < POINTS.length; i++) {
                    // `FaceBakery.rotateVertexBy(vertex, BLOCK_MIDDLE, matriz)`, depois de a rotação do blockstate já estar no vértice (`parent` mais a raiz juntas no estado).
                    Vector3f vertex = new Vector3f(POINTS[i][0], POINTS[i][1], POINTS[i][2]);
                    Vector3f middle = new Vector3f(0.5F, 0.5F, 0.5F);
                    vertex.sub(middle);
                    matrix.transformPosition(vertex);
                    vertex.add(middle);
                    line.append(i == 0 ? "" : ",").append('[').append(POINTS[i][0]).append(',').append(POINTS[i][1]).append(',').append(POINTS[i][2])
                          .append(',').append(vertex.x()).append(',').append(vertex.y()).append(',').append(vertex.z()).append(']');
                }
                line.append("],\"directions\":[");
                int index = 0;
                for (Direction direction : Direction.values()) {
                    line.append(index++ == 0 ? "" : ",").append('"').append(Direction.rotate(matrix, direction).getName()).append('"');
                }
                line.append("]}");
                lines.add(line.toString());
            }
        }
        try {
            Path file = Path.of("trace", "model_transform.jsonl");
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static String quote(String json) {
        return "\"" + json.replace("\\", "\\\\").replace("\"", "\\\"") + "\"";
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
