// A conta do desenho do Fluid Tank do original, reescrita como programa avulso (`java FluidTankRenderReference.java > rastros/fluid_tank_render.jsonl`, sem o jogo): as fórmulas
// são as do Java do Mekanism (`RenderFluidTank.contentsMaxY`, `RenderFluidTank.valveMinY`, `ModelRenderer.getStage`, `MekanismRenderer.getColorARGB(FluidResource, float)`,
// `RenderFluidTank.extractRenderState` e `RenderFluidTankItem.extractArgument`), copiadas linha a linha. Os floats saem pelos bits (`floatToRawIntBits`).
public class FluidTankRenderReference {
    static final int stages = 1_400;
    static final float CONTENTS_MIN_Y = 0.12375F;

    static int as8BitChannel(float value) { return (int) Math.floor(value * 255.0F); }
    static int argbColor(float alpha, int rgb) { return as8BitChannel(alpha) << 24 | rgb & 16777215; }

    static int getColorARGB(int argb, float alpha) {
        if (alpha >= 1) return argb | 0xFF000000;
        else if (alpha <= 0) return argb & 16777215;
        return argbColor(alpha, argb);
    }

    // `MekanismRenderer.getColorARGB(FluidResource, float)`.
    static int fluidColor(boolean empty, int color, boolean lighter, float fluidScale) {
        if (empty) return -1;
        if (lighter) return getColorARGB(color, Math.min(1, fluidScale + 0.2F));
        return color;
    }

    static int getStage(boolean gaseous, int stages, double scale) {
        if (gaseous) return stages - 1;
        return Math.min(stages - 1, (int) (scale * (stages - 1)));
    }

    static float valveMinY(float fluidScale) {
        int stageToUse = Math.min(stages - 1, (int) (fluidScale * (stages - 1)));
        float stageFraction = stageToUse / (float) stages;
        return CONTENTS_MIN_Y + 0.7525F * stageFraction;
    }

    static float contentsMaxY(float fluidScale, boolean gaseous) {
        int stage = getStage(gaseous, stages, fluidScale);
        return CONTENTS_MIN_Y + 0.75225F * (stage / (float) stages);
    }

    // `RenderFluidTank.extractRenderState`, com o estado novo de cada quadro (`createRenderState`: `fluidScale` nasce 0 e a tinta é lida antes de ele ser escrito).
    static void tile(boolean empty, int color, boolean gaseous, float prevScale, boolean valve) {
        float stateFluidScale = 0;
        int tint = fluidColor(empty, color, gaseous, stateFluidScale);
        stateFluidScale = empty ? 0 : prevScale;
        float contentsMaxY = stateFluidScale > 0 ? contentsMaxY(stateFluidScale, gaseous) : 0;
        boolean drawsContents = stateFluidScale > 0 && !empty;
        boolean drawsValve = valve && !gaseous;
        float valveMinY = drawsValve ? valveMinY(stateFluidScale) : 0;
        System.out.println("{\"kind\":\"tile\",\"empty\":" + empty + ",\"color\":" + color + ",\"gaseous\":" + gaseous + ",\"scale\":" + Float.floatToRawIntBits(prevScale)
              + ",\"valve\":" + valve + ",\"tint\":" + tint + ",\"contents\":" + drawsContents + ",\"max_y\":" + Float.floatToRawIntBits(contentsMaxY)
              + ",\"draws_valve\":" + drawsValve + ",\"valve_min_y\":" + Float.floatToRawIntBits(valveMinY) + "}");
    }

    // `RenderFluidTankItem.extractArgument` e o `if` do `submit`.
    static void item(int color, boolean gaseous, long amount, long capacity) {
        float fluidScale = (float) amount / capacity;
        float contentsMaxY = fluidScale > 0 ? contentsMaxY(fluidScale, gaseous) : 0;
        int tint = fluidColor(false, color, gaseous, fluidScale);
        System.out.println("{\"kind\":\"item\",\"color\":" + color + ",\"gaseous\":" + gaseous + ",\"amount\":" + amount + ",\"capacity\":" + capacity
              + ",\"tint\":" + tint + ",\"contents\":" + (contentsMaxY > 0) + ",\"max_y\":" + Float.floatToRawIntBits(contentsMaxY) + "}");
    }

    public static void main(String[] args) {
        float[] scales = {0F, 1.0E-4F, 7.0E-4F, 7.2E-4F, 0.001F, 0.01F, 0.0625F, 0.1F, 0.25F, 1 / 3F, 0.5F, 0.6180339F, 0.75F, 0.9F, 0.99F, 0.9992F, 0.9993F, 0.99999F, 1F, 1.0001F, 1.5F};
        int[] colors = {0xFF3F76E4, 0xFFFFFFFF, 0xFF0D1455, 0x80FFFFFF};
        for (float scale : scales) {
            for (int color : colors) {
                for (int bits = 0; bits < 8; bits++) {
                    tile((bits & 1) != 0, color, (bits & 2) != 0, scale, (bits & 4) != 0);
                }
            }
        }
        // O estágio de todos os 1.400 degraus e a vizinhança de cada um.
        for (int i = 0; i <= 2800; i++) {
            tile(false, 0xFFFFFFFF, false, i / 2800F, true);
        }
        long[] capacities = {32_000, 64_000, 128_000, 256_000, Long.MAX_VALUE, Integer.MAX_VALUE};
        for (long capacity : capacities) {
            long[] amounts = {0, 1, 22, 23, 1000, capacity / 3, capacity / 2, capacity - 1, capacity};
            for (long amount : amounts) {
                for (int color : colors) {
                    item(color, false, amount, capacity);
                    item(color, true, amount, capacity);
                }
            }
        }
    }
}
