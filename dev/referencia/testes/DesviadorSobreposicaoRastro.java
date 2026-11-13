import com.mojang.blaze3d.vertex.PoseStack;
import java.util.Locale;
import net.minecraft.client.renderer.FaceInfo;
import net.minecraft.core.Direction;
import net.minecraft.util.ARGB;
import net.minecraft.util.Mth;
import org.joml.Matrix4f;
import org.joml.Quaternionf;
import org.joml.Vector3f;

/// Os vértices da sobreposição do Configurator (`ConfiguratorOverlayHandler.render`) e a cor do desviador, calculados com as classes reais do Minecraft 26.3 (`PoseStack`, `FaceInfo`,
/// `Direction.fromYRot`, `ARGB.white`) e o JOML: o mesmo `PoseStack` do `render(BlockOutlineRenderState, ...)` (`translate`, `rotateAround` nas faces de cima e de baixo, `scale(0.5F)` e
/// `translate(0.5, 0.5, 0.5)` no desviador) e o mesmo laço de vértices do `render(Pose, VertexConsumer)`. Sem jogo: só as classes de matemática, num `java` comum (JDK 25 ou mais novo).
/// Uso: java -cp mc.jar:joml.jar DesviadorSobreposicaoRastro.java > desviador_sobreposicao.jsonl
public class DesviadorSobreposicaoRastro {

    private static final Quaternionf[] V_ROT = {
          new Quaternionf().setAngleAxis(180 * Mth.DEG_TO_RAD, 0, 1, 0),//south
          new Quaternionf().setAngleAxis(90 * Mth.DEG_TO_RAD, 0, 1, 0),//west
          new Quaternionf().setAngleAxis(270 * Mth.DEG_TO_RAD, 0, 1, 0)//east
    };
    private static final int V_ROT_OFFSET = Direction.SOUTH.ordinal();

    private static String corners(Direction face, float yRot, boolean diversion) {
        PoseStack poseStack = new PoseStack();
        poseStack.pushPose();
        if (face == Direction.UP || face == Direction.DOWN) {
            Direction cameraFacing = Direction.fromYRot(yRot);
            if (cameraFacing != Direction.NORTH) {
                poseStack.rotateAround(V_ROT[cameraFacing.ordinal() - V_ROT_OFFSET], 0.5F, 1, 0.5F);
            }
        }
        if (diversion) {
            poseStack.scale(0.5F, 0.5F, 0.5F);
            poseStack.translate(0.5, 0.5, 0.5);
        }
        Matrix4f matrix = poseStack.last().pose();
        FaceInfo faceInfo = FaceInfo.fromFacing(face);
        StringBuilder out = new StringBuilder("[");
        for (int vertex = 0; vertex < 4; vertex++) {
            FaceInfo.VertexInfo vertInfo = faceInfo.getVertexInfo(vertex);
            float x = vertInfo.xFace().select(0, 0, 0, 1, 1, 1) + face.getStepX() * 0.01F;
            float y = vertInfo.yFace().select(0, 0, 0, 1, 1, 1) + face.getStepY() * 0.01F;
            float z = vertInfo.zFace().select(0, 0, 0, 1, 1, 1) + face.getStepZ() * 0.01F;
            Vector3f at = matrix.transformPosition(x, y, z, new Vector3f());
            float u = vertex < 2 ? 0 : 1;
            float v = vertex == 0 || vertex == 3 ? 0 : 1;
            out.append(vertex == 0 ? "" : ",").append(String.format(Locale.ROOT, "{\"p\":[%s,%s,%s],\"uv\":[%s,%s]}", Float.toString(at.x), Float.toString(at.y), Float.toString(at.z), u, v));
        }
        return out.append("]").toString();
    }

    public static void main(String[] args) {
        float[] yRots = {0F, 44.9F, 45F, 90F, 135F, 180F, 225F, 270F, 315F, -90F, 360F};
        System.out.println(String.format(Locale.ROOT, "{\"meta\":\"ConfiguratorOverlayHandler.render, PoseStack real; diversion_argb=%d\"}", ARGB.white(0.8F)));
        for (Direction face : Direction.values()) {
            for (float yRot : yRots) {
                for (boolean diversion : new boolean[] {false, true}) {
                    System.out.println(String.format(Locale.ROOT, "{\"face\":\"%s\",\"y_rot\":%s,\"diversion\":%s,\"vertices\":%s}", face.getSerializedName(), Float.toString(yRot), diversion, corners(face, yRot, diversion)));
                }
            }
        }
    }
}
