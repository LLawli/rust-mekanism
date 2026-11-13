package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import mekanism.api.MekanismAPI;
import mekanism.api.heat.HeatAPI;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import mekanism.common.Mekanism;
import mekanism.common.tests.MekanismTests;
import net.minecraft.client.CameraType;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundSource;
import mekanism.common.lib.transmitter.DynamicNetwork;
import mekanism.common.lib.transmitter.TransmitterNetworkRegistry;
import net.minecraft.server.permissions.LevelBasedPermissionSet;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.player.ChatVisiblity;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.neoforge.client.event.ClientChatReceivedEvent;
import net.neoforged.neoforge.client.event.ClientTickEvent;

/// O Network Reader no cliente oficial (frente `fase3-faltas`): o chat do clique de cada família, num print de 1920 por 1080, para empilhar com o do port. Só age com o arquivo `cena-leitor` ao
/// lado, uma etapa por linha (`chave=valor` separados por espaço):
///
///   nome=cabo bloco=mekanism:basic_universal_cable baus=0 sneak=0
///   nome=gerador bloco=mekanismgenerators:heat_generator linha=0 sneak=1
///
/// `comando` roda esse comando (o `_` vira espaço, `mek_debug`) como o jogador, com a permissão de dono, no lugar do clique; `feedback=1` liga o `send_command_feedback` antes dele (o padrão da cena é desligado, para o chat
/// só ter o que o clique escreve); `debug=0|1` põe o `MekanismAPI.debug` antes do clique; `alvo=pedra` clica numa pedra posta à direita da linha (um bloco sem block entity).
///
/// `bioma` (um id, `minecraft:desert`) põe esse bioma em volta da linha com o `/fillbiome` antes dos blocos, `y` é a altura da linha (padrão -61) e `ate` os tiques entre pôr os blocos e o clique (padrão
/// 30). Com `bioma` a cena grava no log uma linha `AR {...}` com o nível do mar, o bioma e o `HeatAPI.getAmbientTemp(level, pos)` de cada posição que os três condutores leem (a deles e a de cada vizinho) (o ar
/// que o condutor lê), e toda mensagem de chat que chega ao cliente vira uma linha `CHAT nome: texto`.
///
/// `bloco`: o bloco da linha de três (o clique é no do meio), `linha=0` põe só o bloco no meio, `bau=1` põe um baú no fim da linha, `sneak` agacha o jogador no clique, `espera` os tiques depois do
/// clique até o print (padrão 40). O clique é o de `ServerPlayerGameMode.useItemOn` (o do servidor, como o dos rastros): o chat chega ao cliente e aparece no print.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaLeitor {

    private static final Path ATIVA = Path.of("cena-leitor");
    private static final int X = 300;
    private static final int Y = -61;
    private static final int MIN_Y = -64;
    private static final int MAX_Y = 319;
    private static final int Z = 0;

    private record Etapa(String nome, Map<String, String> campos) {
        String get(String chave, String padrao) {
            return campos.getOrDefault(chave, padrao);
        }
    }

    private static final List<Etapa> etapas = new ArrayList<>();
    private static boolean loaded;
    private static boolean done;
    private static int ticks;
    private static int indice = -1;
    private static int n;
    private static int pausa;
    private static Etapa atual;

    private static void load() {
        try {
            for (String line : Files.readAllLines(ATIVA)) {
                line = line.trim();
                if (line.isEmpty() || line.startsWith("#")) {
                    continue;
                }
                Map<String, String> campos = new java.util.HashMap<>();
                for (String word : line.split("\\s+")) {
                    String[] kv = word.split("=", 2);
                    if (kv.length == 2) {
                        campos.put(kv[0], kv[1]);
                    }
                }
                etapas.add(new Etapa(campos.getOrDefault("nome", "x"), campos));
            }
        } catch (IOException e) {
            Mekanism.logger.error("cena-leitor: leitura", e);
        }
        loaded = true;
    }

    @SuppressWarnings("unchecked")
    private static void limparRedes(MinecraftServer server) {
        try {
            java.lang.reflect.Field field = TransmitterNetworkRegistry.class.getDeclaredField("networks");
            field.setAccessible(true);
            java.util.Set<DynamicNetwork<?, ?, ?>> networks = new java.util.ArrayList<>((java.util.Set<DynamicNetwork<?, ?, ?>>) field.get(null)).stream().collect(java.util.stream.Collectors.toSet());
            Mekanism.logger.info("cena: redes antes da limpeza: {}", networks.size());
            for (DynamicNetwork<?, ?, ?> network : networks) {
                for (Object transmitter : new ArrayList<Object>(network.getTransmitters())) {
                    mekanism.common.content.network.transmitter.Transmitter<?, ?, ?> t = (mekanism.common.content.network.transmitter.Transmitter<?, ?, ?>) transmitter;
                    if (t.getLevel() instanceof net.minecraft.server.level.ServerLevel level) {
                        Mekanism.logger.info("cena: rede velha em {} {}", level.dimension().identifier(), t.getBlockPos());
                        level.setBlockAndUpdate(t.getBlockPos(), net.minecraft.world.level.block.Blocks.AIR.defaultBlockState());
                    }
                }
            }
        } catch (ReflectiveOperationException e) {
            Mekanism.logger.error("cena-leitor: limpeza", e);
        }
    }

    /// O que o original lê como ar: uma linha `AR` por etapa, com o nível do mar, o bioma e o `HeatAPI.getAmbientTemp(level, pos)` das posições que os três condutores leem (as deles e as dos vizinhos).
    private static void gravarAr(MinecraftServer server, String nome, int y) {
        net.minecraft.server.level.ServerLevel level = server.overworld();
        StringBuilder out = new StringBuilder();
        out.append("{\"nome\":\"").append(nome).append("\",\"y\":").append(y).append(",\"sea\":").append(level.getSeaLevel()).append(",\"pos\":[");
        boolean first = true;
        // As posições que um condutor lê: a dele e a de cada vizinho (`getAmbientTemperature(side)` -> `HeatAPI.getAmbientTemp(level, pos.relative(side))`), sem repetir.
        java.util.Set<BlockPos> lidas = new java.util.LinkedHashSet<>();
        for (int i = 0; i < 3; i++) {
            BlockPos conductor = new BlockPos(X + i, y, Z);
            lidas.add(conductor);
            for (Direction side : Direction.values()) {
                lidas.add(conductor.relative(side));
            }
        }
        for (BlockPos pos : lidas) {
            var holder = level.getBiome(pos);
            var biome = holder.value();
            float temperature = biome.getTemperature(pos, level.getSeaLevel());
            out.append(first ? "" : ",").append("{\"at\":[").append(pos.getX()).append(',').append(pos.getY()).append(',').append(pos.getZ()).append("],\"biome\":\"")
                  .append(holder.unwrapKey().map(key -> key.identifier().toString()).orElse("?")).append("\",\"base_bits\":").append(Float.floatToRawIntBits(biome.getBaseTemperature()))
                  .append(",\"modifier\":\"").append(biome.getModifiedClimateSettings().temperatureModifier().getName()).append("\",\"bits\":").append(Float.floatToRawIntBits(temperature))
                  .append(",\"ambient\":").append(HeatAPI.getAmbientTemp(level, pos)).append('}');
            first = false;
        }
        out.append("]}");
        Mekanism.logger.info("AR {}", out);
    }

    /// A cena de uma etapa: o bioma, o chão, o ar, a linha de blocos e o jogador no lugar.
    private static void montar(MinecraftServer server, Etapa e) {
        run(server, "gamerule send_command_feedback false");
        run(server, "gamemode creative @a");
        run(server, "clear @a");
        run(server, "item replace entity @a hotbar.0 with mekanism:network_reader 1");
        // O chão de teste de novo: o `fill` do tique 20 não vale se o chunk ainda não estava carregado.
        run(server, "fill 292 -62 -8 308 -62 12 minecraft:grass_block");
        int y = Integer.parseInt(e.get("y", Integer.toString(Y)));
        if (e.campos().containsKey("bioma")) {
            // O `/fillbiome` age em quadrados de 4 por 4 por 4 e o `BiomeManager` sorteia o quadrado vizinho: a caixa passa da linha em volta, e o `/fillbiome` aceita até 32768 blocos.
            run(server, String.format(Locale.ROOT, "fillbiome %d %d %d %d %d %d %s", X - 8, Math.max(MIN_Y, y - 8), Z - 10, X + 12, Math.min(MAX_Y, y + 8), Z + 10, e.get("bioma", "minecraft:plains")));
        }
        if (y > Y) {
            run(server, String.format(Locale.ROOT, "fill %d %d %d %d %d %d minecraft:stone", X - 4, y - 1, Z - 8, X + 8, y - 1, Z + 10));
        }
        // Abaixo do chão de teste (y menor que -61) a linha fica enterrada e o ar e o jogador ficam em cima do chão: o `fill` de ar não pode levar o chão (o jogador cairia do mundo).
        int topo = Math.max(y, Y);
        run(server, String.format(Locale.ROOT, "fill 296 %d -2 308 %d 6 minecraft:air", topo, topo + 6));
        boolean line = e.get("linha", "1").equals("1");
        for (int i = line ? 0 : 1; i < (line ? 3 : 2); i++) {
            run(server, String.format(Locale.ROOT, "setblock %d %d %d %s", X + i, y, Z, e.get("bloco", "mekanism:basic_universal_cable")));
        }
        if (e.get("bau", "0").equals("1")) {
            run(server, String.format(Locale.ROOT, "setblock %d %d %d minecraft:chest", X + 3, y, Z));
        }
        if (e.get("alvo", "").equals("pedra")) {
            run(server, String.format(Locale.ROOT, "setblock %d %d %d minecraft:stone", X + 3, y, Z));
        }
        run(server, String.format(Locale.ROOT, "tp @a %s %s %s 0 40", X + 1.5 + (e.get("bau", "0").equals("1") ? 1.0 : 0.0), topo + 2.5, Z - 1.4));
        if (e.campos().containsKey("bioma")) {
            gravarAr(server, e.nome(), y);
        }
    }

    @SubscribeEvent
    public static void onChat(ClientChatReceivedEvent event) {
        if (atual != null && Files.exists(ATIVA)) {
            Mekanism.logger.info("CHAT {}: {}", atual.nome(), event.getMessage().getString());
        }
    }

    private static void run(MinecraftServer server, String command) {
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command);
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
            minecraft.options.setCameraType(CameraType.FIRST_PERSON);
            minecraft.options.chatVisibility().set(ChatVisiblity.FULL);
            // A mesma escala de GUI do port (3), para o chat dos dois prints ter o mesmo tamanho.
            minecraft.options.guiScale().set(3);
        }
        minecraft.gui.toastManager().clear();
        if (ticks == 20) {
            server.execute(() -> run(server, "difficulty peaceful"));
            server.execute(() -> run(server, "time set 6000"));
            server.execute(() -> run(server, "weather clear"));
            server.execute(() -> run(server, "gamerule send_command_feedback false"));
            server.execute(() -> run(server, "fill 292 -61 -8 308 -40 12 minecraft:air"));
            server.execute(() -> run(server, "fill 292 -62 -8 308 -62 12 minecraft:grass_block"));
            // Redes que o mundo de teste guarda de outras cenas (o `runs/client` é um só): o registro do `/mek debug` as lista, então elas saem (cada transmissor vira ar) e o log diz quantas eram.
            server.execute(() -> limparRedes(server));
        }
        if (ticks < 40) {
            return;
        }
        if (pausa > 0) {
            pausa--;
            return;
        }
        if (atual == null) {
            indice++;
            if (indice >= etapas.size()) {
                done = true;
                Mekanism.logger.info("cena: fim");
                minecraft.stop();
                return;
            }
            atual = etapas.get(indice);
            // O jogador vai para a cena antes de tudo e a cena espera 80 tiques: o chunk da linha precisa estar carregado (o mundo `runs/client` reabre com o jogador onde a última cena o deixou, ou no spawn).
            n = -80;
            final Etapa e = atual;
            server.execute(() -> run(server, String.format(Locale.ROOT, "tp @a %s %s %s 0 40", X + 1.5, Math.max(Integer.parseInt(e.get("y", Integer.toString(Y))), Y) + 2.5, Z - 1.4)));
            return;
        }
        n++;
        if (n == 0) {
            final Etapa e = atual;
            server.execute(() -> montar(server, e));
        }
        if (n == Integer.parseInt(atual.get("ate", "30"))) {
            final Etapa e = atual;
            server.execute(() -> {
                ServerPlayer player = server.getPlayerList().getPlayers().getFirst();
                if (e.campos().containsKey("debug")) {
                    MekanismAPI.debug = e.get("debug", "0").equals("1");
                }
                if (!e.get("comando", "").isEmpty()) {
                    if (e.get("feedback", "0").equals("1")) {
                        run(server, "gamerule send_command_feedback true");
                    }
                    server.getCommands().performPrefixedCommand(player.createCommandSourceStack().withPermission(LevelBasedPermissionSet.OWNER), e.get("comando", "").replace('_', ' '));
                    return;
                }
                player.setShiftKeyDown(e.get("sneak", "0").equals("1"));
                int dx = e.get("alvo", "bau").equals("pedra") || (e.get("bau", "0").equals("1") && e.get("alvo", "bau").equals("bau")) ? 3 : 1;
                BlockPos pos = new BlockPos(X + dx, Integer.parseInt(e.get("y", Integer.toString(Y))), Z);
                BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(pos).relative(Direction.NORTH, 0.5), Direction.NORTH, pos, false);
                player.gameMode.useItemOn(player, player.level(), player.getItemInHand(InteractionHand.MAIN_HAND), InteractionHand.MAIN_HAND, hit);
                player.setShiftKeyDown(false);
            });
        }
        if (n == Integer.parseInt(atual.get("ate", "30")) + Integer.parseInt(atual.get("espera", "40"))) {
            Screenshot.grab(minecraft.gameDirectory, "leitor_oficial_" + atual.nome() + ".png", minecraft.gameRenderer.mainRenderTarget(), 1,
                  message -> Mekanism.logger.info("cena: {}", message.getString()));
            atual = null;
            pausa = 10;
        }
    }
}
