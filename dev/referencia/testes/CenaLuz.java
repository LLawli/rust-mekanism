package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.common.Mekanism;
import mekanism.common.tests.MekanismTests;
import com.mojang.blaze3d.vertex.QuadInstance;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.block.BlockModelLighter;
import net.minecraft.client.renderer.block.dispatch.BlockStateModel;
import net.minecraft.client.renderer.block.dispatch.BlockStateModelPart;
import net.minecraft.client.resources.model.geometry.BakedQuad;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.MinecraftServer;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.model.ao.EnhancedBlockModelLighter;

/// O sombreamento de vértice (a cor cinza do AO e a luz suave) que o NeoForge dá aos quads de um bloco, comparado ao do `BlockModelLighter` do vanilla, no mesmo mundo: o rastro de
/// `rastros/lighting_enhanced.jsonl`, que `machinery_basis` lê em `tests/enhanced_lighting_module.rs`. Só age com o arquivo `cena-luz` na pasta do jogo (um bloco por linha, `id`).
///
/// Cada bloco vai para `(X0 + 8 * i, -60, Z0)`, com uma pedra encostada a leste e outra na diagonal de cima a oeste (para o AO e a luz diferirem de um lado a outro), sobre o chão
/// do superplano. Para cada quad de cada parte do modelo (cada direção de corte e os sem corte) que usa AO, grava uma linha: o quad (posições, direção, normais, `shade`), a cor
/// e a luz dos quatro vértices pelo vanilla e pelo `EnhancedBlockModelLighter`; e uma linha `mundo` com as 27 células em volta do bloco (a luz de cada uma, o brilho de sombra,
/// se deixa a luz passar, se tem colisão de bloco cheio).
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaLuz {

    private static final Path ATIVA = Path.of("cena-luz");
    private static final Path SAIDA = Path.of("luz_trace.jsonl");
    private static final int X0 = 60;
    private static final int Y = -60;
    private static final int Z0 = 100;
    private static final int PASSO = 8;

    private static List<String> blocos;
    private static int ticks;

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (minecraft.level == null || minecraft.player == null || server == null || !Files.exists(ATIVA)) {
            return;
        }
        if (blocos == null) {
            try {
                blocos = Files.readAllLines(ATIVA).stream().map(String::trim).filter(l -> !l.isEmpty() && !l.startsWith("#")).toList();
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
        ticks++;
        if (ticks == 20) {
            for (String comando : new String[] {"difficulty peaceful", "time set 6000", "weather clear", "gamemode creative", "tp @a 80 -58 94 0 20",
                  String.format("fill %d %d %d %d %d %d minecraft:air", X0 - 4, Y, Z0 - 4, X0 + PASSO * blocos.size() + 4, Y + 8, Z0 + 4)}) {
                run(server, comando);
            }
        }
        if (ticks == 60) {
            for (int i = 0; i < blocos.size(); i++) {
                int x = X0 + PASSO * i;
                run(server, String.format("setblock %d %d %d %s", x, Y, Z0, blocos.get(i)));
                run(server, String.format("setblock %d %d %d minecraft:stone", x + 1, Y, Z0));
                run(server, String.format("setblock %d %d %d minecraft:stone", x - 1, Y + 1, Z0));
            }
        }
        if (ticks == 200) {
            try {
                Files.writeString(SAIDA, dump(minecraft));
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
            Mekanism.logger.info("cena-luz: gravado {}", SAIDA.toAbsolutePath());
        }
        if (ticks == 230) {
            minecraft.stop();
        }
    }

    private static String dump(Minecraft minecraft) {
        StringBuilder out = new StringBuilder();
        var level = minecraft.level;
        BlockModelLighter vanilla = new BlockModelLighter();
        BlockModelLighter enhanced = new EnhancedBlockModelLighter();
        RandomSource random = RandomSource.create(42L);
        QuadInstance instance = new QuadInstance();
        for (int i = 0; i < blocos.size(); i++) {
            BlockPos pos = new BlockPos(X0 + PASSO * i, Y, Z0);
            BlockState state = level.getBlockState(pos);
            String id = BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString();
            if (!id.equals(blocos.get(i).replaceAll("\\[.*", ""))) {
                throw new IllegalStateException("o bloco não foi posto: " + id + " em vez de " + blocos.get(i));
            }
            JsonObject world = new JsonObject();
            world.addProperty("tipo", "mundo");
            world.addProperty("bloco", blocos.get(i));
            world.addProperty("cheio", state.isCollisionShapeFullBlock(level, pos));
            JsonArray cells = new JsonArray();
            for (int dy = -1; dy <= 1; dy++) {
                for (int dz = -1; dz <= 1; dz++) {
                    for (int dx = -1; dx <= 1; dx++) {
                        BlockPos p = pos.offset(dx, dy, dz);
                        BlockState s = level.getBlockState(p);
                        JsonObject cell = new JsonObject();
                        cell.addProperty("luz", vanilla.getLightCoords(s, level, p));
                        cell.addProperty("sombra", s.getShadeBrightness(level, p));
                        cell.addProperty("passa", s.isLightPermeable());
                        cells.add(cell);
                    }
                }
            }
            world.add("celulas", cells);
            out.append(world).append('\n');
            BlockStateModel model = minecraft.getModelManager().getBlockStateModelSet().get(state);
            List<BlockStateModelPart> parts = new ArrayList<>();
            model.collectParts(level, pos, state, random, parts);
            vanilla.reset();
            enhanced.reset();
            for (BlockStateModelPart part : parts) {
                boolean ao = switch (part.ambientOcclusion()) {
                    case TRUE -> true;
                    case DEFAULT -> state.getLightEmission(level, pos) == 0;
                    case FALSE -> false;
                };
                // A ordem do `ModelBlockRenderer`: as seis direções de corte e, por último, os quads sem corte (o cache de faces do NeoForge depende dela).
                for (int d = 0; d <= 6; d++) {
                    Direction corte = d < 6 ? Direction.values()[d] : null;
                    for (BakedQuad quad : part.getQuads(corte)) {
                        JsonObject row = new JsonObject();
                        row.addProperty("tipo", "quad");
                        row.addProperty("bloco", blocos.get(i));
                        row.addProperty("corte", corte == null ? "" : corte.getName());
                        row.addProperty("direcao", quad.direction().getName());
                        row.addProperty("ao", ao && quad.materialInfo().ambientOcclusion());
                        row.addProperty("sombra_forcada", quad.materialInfo().shadeDirectionOverride() == null ? "" : quad.materialInfo().shadeDirectionOverride().getName());
                        JsonArray positions = new JsonArray();
                        JsonArray normals = new JsonArray();
                        for (int v = 0; v < 4; v++) {
                            JsonArray p = new JsonArray();
                            p.add(quad.position(v).x());
                            p.add(quad.position(v).y());
                            p.add(quad.position(v).z());
                            positions.add(p);
                            normals.add(quad.bakedNormals().normal(v));
                        }
                        row.add("posicoes", positions);
                        row.add("normais", normals);
                        if (!(ao && quad.materialInfo().ambientOcclusion())) {
                            out.append(row).append('\n');
                            continue;
                        }
                        vanilla.prepareQuadAmbientOcclusion(level, state, pos, quad, instance);
                        row.add("vanilla", vertices(instance));
                        enhanced.prepareQuadAmbientOcclusion(level, state, pos, quad, instance);
                        row.add("neoforge", vertices(instance));
                        out.append(row).append('\n');
                    }
                }
            }
        }
        return out.toString();
    }

    private static JsonObject vertices(QuadInstance instance) {
        JsonArray colors = new JsonArray();
        JsonArray lights = new JsonArray();
        for (int v = 0; v < 4; v++) {
            colors.add(instance.getColor(v));
            lights.add(instance.getLightCoords(v));
        }
        JsonObject out = new JsonObject();
        out.add("cor", colors);
        out.add("luz", lights);
        return out;
    }

    private static void run(MinecraftServer server, String command) {
        server.execute(() -> server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command));
    }
}
