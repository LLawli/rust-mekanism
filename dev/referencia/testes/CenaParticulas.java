package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Queue;
import java.util.TreeMap;
import mekanism.common.Mekanism;
import mekanism.common.block.attribute.Attribute;
import mekanism.common.capabilities.energy.BasicEnergyContainer;
import mekanism.common.config.MekanismConfig;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.generators.common.registries.GeneratorsBlocks;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.particle.Particle;
import net.minecraft.client.particle.ParticleEngine;
import net.minecraft.client.particle.ParticleGroup;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.MinecraftServer;
import net.minecraft.sounds.SoundSource;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.transfer.item.ItemResource;

/// As partículas da máquina acesa no cliente oficial (`BlockTile.animateTick`, `BlockTile.java:80-103`; `AttributeParticleFX`, `Machine.java:23` e
/// `GeneratorsBlockTypes.java:69`). Só age com o arquivo `cena-particulas` ao lado, uma linha por etapa, em pares `chave=valor`:
///
///   rastro semente=12345 chamadas=6
///   taxa nome=chamber maquina=chamber facing=south jogador=0,-0.5,6 giro=180,5 ticks=600 prints=200,201
///
/// `rastro` chama o `animateTick` do bloco direto, com uma `RandomSource.create(semente)` por (máquina, direção), `chamadas` vezes seguidas, e grava
/// cada partícula que o `ClientLevel.addParticle` entregou ao `ParticleEngine` (a classe e a posição, lidas da fila `particlesToAdd`): uma linha por
/// chamada. `taxa` põe a máquina acesa de verdade (item e energia, como a `CenaSomMedida`), deixa o cliente sortear sozinho (`ClientLevel.animateTick`)
/// e conta, a cada tick, as partículas novas (as de `age` zero) por classe; uma linha por segundo e uma de resumo. `prints=N,M` tira um print nos ticks
/// N e M da etapa. `jogador` são os PÉS do jogador relativos ao centro do bloco, `giro` o `yaw,pitch`. `efeitos=false` desliga `machineEffects`.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaParticulas {

    private static final Path ATIVA = Path.of("cena-particulas");
    private static final Path SAIDA = Path.of("particulas.jsonl");
    private static final int X = 300;
    private static final int Y = -60;
    private static final int Z = 3;
    private static final double CX = X + 0.5;
    private static final double CY = Y + 0.5;
    private static final double CZ = Z + 0.5;
    private static final int PAUSA = 10;

    private static final class Etapa {
        String tipo = "";
        String nome = "x";
        String maquina = "chamber";
        Direction facing = Direction.SOUTH;
        long semente = 12345;
        int chamadas = 6;
        double[] jogador = {0.0, -0.5, 6.0};
        float yaw = 180;
        float pitch = 5;
        int ticks = 600;
        boolean efeitos = true;
        final List<Integer> prints = new ArrayList<>();
        final Map<String, Integer> totais = new TreeMap<>();
        final Map<String, Integer> segundo = new TreeMap<>();
        int ativos;
    }

    private static final List<Etapa> etapas = new ArrayList<>();
    private static boolean loaded;
    private static boolean done;
    private static int ticks;
    private static int indice = -1;
    private static int n;
    private static int pausa;
    private static Etapa atual;

    private static void escreve(String linha) {
        try {
            Files.writeString(SAIDA, linha + "\n", StandardOpenOption.CREATE, StandardOpenOption.APPEND);
        } catch (IOException e) {
            Mekanism.logger.error("cena-particulas: escrita", e);
        }
    }

    private static void load() {
        try {
            for (String line : Files.readAllLines(ATIVA)) {
                line = line.trim();
                if (line.isEmpty() || line.startsWith("#")) {
                    continue;
                }
                String[] words = line.split("\\s+");
                Etapa e = new Etapa();
                e.tipo = words[0];
                for (int i = 1; i < words.length; i++) {
                    String[] kv = words[i].split("=", 2);
                    if (kv.length < 2) {
                        continue;
                    }
                    switch (kv[0]) {
                        case "nome" -> e.nome = kv[1];
                        case "maquina" -> e.maquina = kv[1];
                        case "facing" -> e.facing = Direction.byName(kv[1]);
                        case "semente" -> e.semente = Long.parseLong(kv[1]);
                        case "chamadas" -> e.chamadas = Integer.parseInt(kv[1]);
                        case "jogador" -> e.jogador = parseVec(kv[1]);
                        case "giro" -> {
                            String[] g = kv[1].split(",");
                            e.yaw = Float.parseFloat(g[0]);
                            e.pitch = Float.parseFloat(g[1]);
                        }
                        case "ticks" -> e.ticks = Integer.parseInt(kv[1]);
                        case "efeitos" -> e.efeitos = Boolean.parseBoolean(kv[1]);
                        case "prints" -> {
                            for (String p : kv[1].split(",")) {
                                e.prints.add(Integer.parseInt(p));
                            }
                        }
                        default -> {
                        }
                    }
                }
                etapas.add(e);
            }
        } catch (IOException e) {
            Mekanism.logger.error("cena-particulas: sem arquivo", e);
        }
        loaded = true;
        try {
            Files.deleteIfExists(SAIDA);
        } catch (IOException e) {
            Mekanism.logger.error("cena-particulas: limpar", e);
        }
    }

    private static double[] parseVec(String text) {
        String[] p = text.split(",");
        return new double[]{Double.parseDouble(p[0]), Double.parseDouble(p[1]), Double.parseDouble(p[2])};
    }

    private static BlockState block(String machine) {
        return switch (machine) {
            case "crusher" -> MekanismBlocks.CRUSHER.defaultState();
            case "smelter" -> MekanismBlocks.ENERGIZED_SMELTER.defaultState();
            case "infuser" -> MekanismBlocks.METALLURGIC_INFUSER.defaultState();
            case "heat" -> GeneratorsBlocks.HEAT_GENERATOR.defaultState();
            default -> MekanismBlocks.ENRICHMENT_CHAMBER.defaultState();
        };
    }

    private static void run(MinecraftServer server, String command) {
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command);
    }

    private static TileEntityMekanism tile(MinecraftServer server) {
        return (TileEntityMekanism) server.overworld().getBlockEntity(new BlockPos(X, Y, Z));
    }

    /// Liga a máquina (item na entrada e energia cheia), ou o Heat Generator (carvão no slot de combustível).
    private static void energia(MinecraftServer server, Etapa e) {
        TileEntityMekanism tile = tile(server);
        if (tile == null) {
            return;
        }
        if (e.maquina.equals("heat")) {
            var coal = BuiltInRegistries.ITEM.getValue(Identifier.parse("minecraft:coal"));
            tile.getInventorySlots().get(0).setContents(ItemResource.of(coal), 64, null);
            // A energia cheia pára o gerador: esvazia o contêiner a cada chamada (um consumidor encostado faria o mesmo).
            ((BasicEnergyContainer) tile.getEnergyContainer()).setEnergy(0, null);
            return;
        }
        BasicEnergyContainer container = (BasicEnergyContainer) tile.getEnergyContainer();
        boolean infuser = e.maquina.equals("infuser");
        String input = switch (e.maquina) {
            case "crusher" -> "minecraft:cobblestone";
            case "smelter" -> "minecraft:raw_iron";
            case "infuser" -> "minecraft:iron_ingot";
            default -> "minecraft:coal";
        };
        var item = BuiltInRegistries.ITEM.getValue(Identifier.parse(input));
        tile.getInventorySlots().get(infuser ? 1 : 0).setContents(ItemResource.of(item), 64, null);
        if (infuser) {
            var coal = BuiltInRegistries.ITEM.getValue(Identifier.parse("minecraft:coal"));
            tile.getInventorySlots().get(0).setContents(ItemResource.of(coal), 64, null);
        }
        container.setEnergy(container.getCapacityAsLong(), null);
    }

    private static void teleport(MinecraftServer server, Etapa e) {
        run(server, String.format(Locale.ROOT, "tp @a %.4f %.4f %.4f %.1f %.1f", CX + e.jogador[0], CY + e.jogador[1], CZ + e.jogador[2], e.yaw, e.pitch));
    }

    private static Object field(Class<?> type, Object target, String name) {
        try {
            Field field = type.getDeclaredField(name);
            field.setAccessible(true);
            return field.get(target);
        } catch (ReflectiveOperationException ex) {
            throw new IllegalStateException(ex);
        }
    }

    @SuppressWarnings("unchecked")
    private static Queue<Particle> fila(ParticleEngine engine) {
        return (Queue<Particle>) field(ParticleEngine.class, engine, "particlesToAdd");
    }

    @SuppressWarnings("unchecked")
    private static Map<?, ParticleGroup<?>> grupos(ParticleEngine engine) {
        return (Map<?, ParticleGroup<?>>) field(ParticleEngine.class, engine, "particles");
    }

    private static String num(double v) {
        return Double.toString(v);
    }

    /// O `animateTick` do bloco, direto, com a semente fixa: o que a classe `BlockTile` entrega a `Level.addParticle`.
    private static void rastro(Minecraft minecraft, Etapa e) {
        ParticleEngine engine = minecraft.particleEngine;
        String[] maquinas = {"chamber", "crusher", "smelter", "infuser", "heat"};
        Direction[] direcoes = {Direction.NORTH, Direction.SOUTH, Direction.WEST, Direction.EAST};
        BlockPos pos = new BlockPos(X, Y, Z);
        for (boolean ativo : new boolean[]{true, false}) {
            for (boolean efeitos : new boolean[]{true, false}) {
                MekanismConfig.client.machineEffects.set(efeitos);
                for (String maquina : maquinas) {
                    for (Direction facing : direcoes) {
                        BlockState state = Attribute.setFacing(block(maquina), facing);
                        state = Attribute.setActive(state, ativo);
                        RandomSource random = RandomSource.create(e.semente);
                        for (int chamada = 0; chamada < e.chamadas; chamada++) {
                            fila(engine).clear();
                            state.getBlock().animateTick(state, minecraft.level, pos, random);
                            StringBuilder lista = new StringBuilder();
                            for (Particle p : fila(engine)) {
                                if (lista.length() > 0) {
                                    lista.append(',');
                                }
                                lista.append(String.format(Locale.ROOT, "{\"classe\":\"%s\",\"x\":%s,\"y\":%s,\"z\":%s}", p.getClass().getSimpleName(),
                                      num((double) field(Particle.class, p, "x")), num((double) field(Particle.class, p, "y")), num((double) field(Particle.class, p, "z"))));
                            }
                            escreve(String.format(Locale.ROOT,
                                  "{\"e\":\"rastro\",\"maquina\":\"%s\",\"facing\":\"%s\",\"semente\":%d,\"chamada\":%d,\"ativo\":%s,\"efeitos\":%s,\"x\":%d,\"y\":%d,\"z\":%d,\"particulas\":[%s]}",
                                  maquina, facing.getName(), e.semente, chamada, ativo, efeitos, X, Y, Z, lista));
                        }
                    }
                }
            }
        }
        fila(engine).clear();
        MekanismConfig.client.machineEffects.set(true);
    }

    /// Conta as partículas novas deste tick: as que ainda não foram ticadas (`age` zero), em todos os grupos, por classe.
    private static Map<String, Integer> novas(Minecraft minecraft) {
        Map<String, Integer> count = new TreeMap<>();
        ParticleEngine engine = minecraft.particleEngine;
        for (ParticleGroup<?> group : grupos(engine).values()) {
            @SuppressWarnings("unchecked")
            Queue<Particle> particles = (Queue<Particle>) field(ParticleGroup.class, group, "particles");
            for (Particle p : particles) {
                if ((int) field(Particle.class, p, "age") == 0) {
                    count.merge(p.getClass().getSimpleName(), 1, Integer::sum);
                }
            }
        }
        for (Particle p : fila(engine)) {
            count.merge(p.getClass().getSimpleName(), 1, Integer::sum);
        }
        return count;
    }

    private static String json(Map<String, Integer> map) {
        StringBuilder out = new StringBuilder("{");
        for (var entry : map.entrySet()) {
            if (out.length() > 1) {
                out.append(',');
            }
            out.append('"').append(entry.getKey()).append("\":").append(entry.getValue());
        }
        return out.append('}').toString();
    }

    private static void shot(Minecraft minecraft, String name) {
        Screenshot.grab(minecraft.gameDirectory, name + ".png", minecraft.gameRenderer.mainRenderTarget(), 1, message -> Mekanism.logger.info("cena: {}", message.getString()));
    }

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (done || minecraft.level == null || minecraft.player == null || server == null || !Files.exists(ATIVA)) {
            return;
        }
        if (!loaded) {
            load();
        }
        ticks++;
        if (ticks == 1) {
            minecraft.options.getSoundSourceOptionInstance(SoundSource.MUSIC).set(0.0);
            minecraft.options.getSoundSourceOptionInstance(SoundSource.AMBIENT).set(0.0);
        }
        if (ticks == 20) {
            server.execute(() -> run(server, "difficulty peaceful"));
            server.execute(() -> run(server, "time set 6000"));
            server.execute(() -> run(server, "weather clear"));
            server.execute(() -> run(server, "gamerule send_command_feedback false"));
            server.execute(() -> run(server, "gamemode creative @a"));
            server.execute(() -> run(server, "clear @a"));
        }
        if (ticks < 40) {
            return;
        }
        if (atual == null) {
            if (pausa > 0) {
                pausa--;
                return;
            }
            indice++;
            if (indice >= etapas.size()) {
                done = true;
                escreve("{\"e\":\"fim\"}");
                minecraft.stop();
                return;
            }
            atual = etapas.get(indice);
            n = 0;
            final Etapa e = atual;
            if (e.tipo.equals("rastro")) {
                server.execute(() -> teleport(server, e));
                escreve(String.format(Locale.ROOT, "{\"e\":\"etapa\",\"tipo\":\"rastro\"}"));
                return;
            }
            MekanismConfig.client.machineEffects.set(e.efeitos);
            server.execute(() -> {
                run(server, "fill " + (X - 8) + " -61 -8 " + (X + 8) + " -40 12 minecraft:air");
                run(server, "fill " + (X - 8) + " -62 -8 " + (X + 8) + " -62 12 minecraft:stone");
                server.overworld().setBlockAndUpdate(new BlockPos(X, Y, Z), Blocks.AIR.defaultBlockState());
                BlockState state = Attribute.setFacing(block(e.maquina), e.facing);
                server.overworld().setBlockAndUpdate(new BlockPos(X, Y, Z), state);
                TileEntityMekanism tile = tile(server);
                tile.applyComponents(net.minecraft.core.component.DataComponentMap.EMPTY, net.minecraft.core.component.DataComponentPatch.EMPTY);
                teleport(server, e);
            });
            escreve(String.format(Locale.ROOT, "{\"e\":\"etapa\",\"tipo\":\"taxa\",\"nome\":\"%s\",\"maquina\":\"%s\",\"facing\":\"%s\",\"efeitos\":%s,\"ticks\":%d}",
                  e.nome, e.maquina, e.facing.getName(), e.efeitos, e.ticks));
            return;
        }
        final Etapa e = atual;
        n++;
        if (e.tipo.equals("rastro")) {
            // Dá tempo ao teleporte e à câmera (o limite de 32 blocos do `doAddParticle`).
            if (n == 20) {
                rastro(minecraft, e);
                escreve("{\"e\":\"etapa_fim\",\"tipo\":\"rastro\"}");
                atual = null;
                pausa = PAUSA;
            }
            return;
        }
        server.execute(() -> teleport(server, e));
        if (n == 2 || n % 100 == 0 || (e.maquina.equals("heat") && n % 5 == 0)) {
            server.execute(() -> energia(server, e));
        }
        // A contagem só começa com a máquina acesa (a primeira operação leva alguns ticks para acender o bloco).
        BlockState state = minecraft.level.getBlockState(new BlockPos(X, Y, Z));
        boolean aceso = Attribute.isActive(state);
        if (aceso) {
            e.ativos++;
            Map<String, Integer> novas = novas(minecraft);
            for (var entry : novas.entrySet()) {
                e.totais.merge(entry.getKey(), entry.getValue(), Integer::sum);
                e.segundo.merge(entry.getKey(), entry.getValue(), Integer::sum);
            }
            if (e.ativos % 20 == 0) {
                escreve(String.format(Locale.ROOT, "{\"e\":\"segundo\",\"nome\":\"%s\",\"n\":%d,\"ativos\":%d,\"novas\":%s}", e.nome, n, e.ativos, json(e.segundo)));
                e.segundo.clear();
            }
        }
        if (e.prints.contains(n)) {
            shot(minecraft, "particulas_oficial_" + e.nome + "_" + n);
        }
        if (n >= e.ticks) {
            escreve(String.format(Locale.ROOT, "{\"e\":\"resumo\",\"nome\":\"%s\",\"ticks\":%d,\"ativos\":%d,\"totais\":%s}", e.nome, n, e.ativos, json(e.totais)));
            server.execute(() -> server.overworld().setBlockAndUpdate(new BlockPos(X, Y, Z), Blocks.AIR.defaultBlockState()));
            atual = null;
            pausa = PAUSA;
        }
    }
}
