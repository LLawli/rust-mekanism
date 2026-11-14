// A conta do conteúdo dos transmissores do original, reescrita como programa avulso (`java TransmitterContentsReference.java > rastros/transmitter_contents.jsonl`, sem o jogo):
// as fórmulas são as do Java do Mekanism (`HeatUtils.getColorFromTemp`, `Color.blendOnto`, `ARGB`, `MekanismRenderer.getColorARGB`, `ModelRenderer.getStage`, `RenderMechanicalPipe`,
// `RenderPressurizedTube`, `RenderUniversalCable`, `LightCoordsUtil.lightCoordsWithEmission`), copiadas linha a linha. O `colorCache` é do processo, como no original: a primeira
// temperatura que cai num balde de 100 K fixa o alfa dele.
import java.util.HashMap;
import java.util.Map;

public class TransmitterContentsReference {
    static final double AMBIENT_TEMP = 300;
    static final Map<Integer, double[]> colorCache = new HashMap<>();

    static int as8BitChannel(float value) { return (int) Math.floor(value * 255.0F); }
    static int argbColor(float alpha, int rgb) { return as8BitChannel(alpha) << 24 | rgb & 16777215; }
    static int white(float alpha) { return as8BitChannel(alpha) << 24 | 16777215; }
    static int opaque(int c) { return c | 0xFF000000; }
    static int transparent(int c) { return c & 16777215; }

    static int getColorARGB(int argb, float alpha) {
        if (alpha >= 1) return opaque(argb);
        else if (alpha <= 0) return transparent(argb);
        return argbColor(alpha, argb);
    }

    static int fluidTint(int color, boolean lighter, float scale) {
        if (lighter) return getColorARGB(color, Math.min(1, scale + 0.2F));
        return color;
    }

    static int stage(boolean gaseous, int stages, double scale) {
        if (gaseous) return stages - 1;
        return Math.min(stages - 1, (int) (scale * (stages - 1)));
    }

    static int lightWithEmission(int sky, int block, int emission) {
        if (emission == 0) return sky | block << 16;
        return Math.max(block, emission) << 16 | Math.max(sky, emission);
    }

    static double[] blendOnto(double[] s, double[] d) {
        double sR = s[0], sG = s[1], sB = s[2], sA = s[3];
        double dR = d[0], dG = d[1], dB = d[2], dA = d[3];
        return new double[]{sR * sA + dR * (1 - sA), sG * sA + dG * (1 - sA), sB * sA + dB * (1 - sA), dA + sA * (1 - dA)};
    }

    static int round8(double v) { return (int) Math.round(v * 0xFF); }

    static int argb(double[] c) { return round8(c[3]) << 24 | (round8(c[0]) & 0xFF) << 16 | (round8(c[1]) & 0xFF) << 8 | (round8(c[2]) & 0xFF); }

    static double[] getColorFromTemp(double temperature, double[] baseColor) {
        double absTemp = temperature + AMBIENT_TEMP;
        absTemp /= 100;
        if (colorCache.containsKey((int) absTemp)) return blendOnto(colorCache.get((int) absTemp), baseColor);
        double tmpCalc, red, blue;
        double effectiveTemp = absTemp;
        if (effectiveTemp < 10) effectiveTemp = 10;
        if (effectiveTemp > 400) effectiveTemp = 400;
        if (effectiveTemp <= 66) red = 1;
        else {
            tmpCalc = effectiveTemp - 60;
            tmpCalc = 329.698727446 * Math.pow(tmpCalc, -0.1332047592);
            red = tmpCalc / 255D;
        }
        if (effectiveTemp <= 66) {
            tmpCalc = effectiveTemp;
            tmpCalc = 99.4708025861 * Math.log(tmpCalc) - 161.1195681661;
        } else {
            tmpCalc = effectiveTemp - 60;
            tmpCalc = 288.1221695283 * Math.pow(tmpCalc, -0.0755148492);
        }
        double green = tmpCalc / 255D;
        if (effectiveTemp >= 66) blue = 1;
        else if (effectiveTemp <= 19) blue = 0;
        else {
            tmpCalc = effectiveTemp - 10;
            tmpCalc = 138.5177312231 * Math.log(tmpCalc) - 305.0447927307;
            blue = tmpCalc / 255D;
        }
        double alpha = temperature / 1_000;
        red = Math.clamp(red, 0, 1);
        green = Math.clamp(green, 0, 1);
        blue = Math.clamp(blue, 0, 1);
        alpha = Math.clamp(alpha, 0, 1);
        double[] colorTemperature = new double[]{red, green, blue, alpha};
        colorCache.put((int) absTemp, colorTemperature);
        return blendOnto(colorTemperature, baseColor);
    }

    public static void main(String[] args) {
        float[] scales = {0f, 0.001f, 0.004f, 0.01f, 0.1f, 0.2f, 0.25f, 0.3333333f, 0.5f, 0.6f, 0.75f, 0.8f, 0.9f, 0.99f, 0.995f, 1f};
        for (float scale : scales) {
            System.out.println("{\"kind\":\"scale\",\"scale\":" + scale + ",\"energy\":" + white(scale)
                  + ",\"tube\":" + argbColor(Math.max(0.2F, scale), 0xF2CD67)
                  + ",\"gas\":" + fluidTint(0xFFFFFF, true, scale)
                  + ",\"gas_tint\":" + fluidTint(0x3366CC, true, scale)
                  + ",\"liquid\":" + fluidTint(0xFFFFFF, false, scale)
                  + ",\"stage\":" + Math.max(3, stage(false, 100, scale))
                  + ",\"stage_gas\":" + Math.max(3, stage(true, 100, scale)) + "}");
        }
        int[][] light = {{0, 0, 0}, {15, 0, 0}, {3, 7, 0}, {3, 7, 15}, {15, 15, 15}, {0, 2, 9}};
        for (int[] l : light) {
            System.out.println("{\"kind\":\"light\",\"sky\":" + l[0] + ",\"block\":" + l[1] + ",\"emission\":" + l[2] + ",\"packed\":" + lightWithEmission(l[0], l[1], l[2]) + "}");
        }
        double[] base = {0.2, 0.2, 0.2, 1};
        double[] temps = {300, 0, -50, 100, 300, 301, 350, 500, 799, 800, 1000, 1500, 2000, 3000, 5000, 10000, 20000, 39000, 40000, 100000, 1000000, 300, 1000, 2000, 5, 12345.678};
        for (double t : temps) {
            double[] c = getColorFromTemp(t, base);
            System.out.println("{\"kind\":\"heat\",\"temperature\":" + t + ",\"argb\":" + argb(c) + "}");
        }
    }
}
