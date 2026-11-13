package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import mekanism.api.MekanismAPI;
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
/// `bloco`: o bloco da linha de três (o clique é no do meio), `linha=0` põe só o bloco no meio, `bau=1` põe um baú no fim da linha, `sneak` agacha o jogador no clique, `espera` os tiques depois do
/// clique até o print (padrão 40). O clique é o de `ServerPlayerGameMode.useItemOn` (o do servidor, como o dos rastros): o chat chega ao cliente e aparece no print.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaLeitor {

    private static final Path ATIVA = Path.of("cena-leitor");
    private static final int X = 300;
    private static final int Y = -61;
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
            n = 0;
            final Etapa e = atual;
            server.execute(() -> {
                run(server, "gamerule send_command_feedback false");
                run(server, "gamemode creative @a");
                run(server, "clear @a");
                run(server, "item replace entity @a hotbar.0 with mekanism:network_reader 1");
                run(server, "fill 296 -61 -2 308 -55 6 minecraft:air");
                boolean line = e.get("linha", "1").equals("1");
                for (int i = line ? 0 : 1; i < (line ? 3 : 2); i++) {
                    run(server, String.format(Locale.ROOT, "setblock %d %d %d %s", X + i, Y, Z, e.get("bloco", "mekanism:basic_universal_cable")));
                }
                if (e.get("bau", "0").equals("1")) {
                    run(server, String.format(Locale.ROOT, "setblock %d %d %d minecraft:chest", X + 3, Y, Z));
                }
                if (e.get("alvo", "").equals("pedra")) {
                    run(server, String.format(Locale.ROOT, "setblock %d %d %d minecraft:stone", X + 3, Y, Z));
                }
                run(server, String.format(Locale.ROOT, "tp @a %s %s %s 0 40", X + 1.5 + (e.get("bau", "0").equals("1") ? 1.0 : 0.0), Y + 2.5, Z - 1.4));
            });
            return;
        }
        n++;
        if (n == 30) {
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
                BlockPos pos = new BlockPos(X + dx, Y, Z);
                BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(pos).relative(Direction.NORTH, 0.5), Direction.NORTH, pos, false);
                player.gameMode.useItemOn(player, player.level(), player.getItemInHand(InteractionHand.MAIN_HAND), InteractionHand.MAIN_HAND, hit);
                player.setShiftKeyDown(false);
            });
        }
        if (n == 30 + Integer.parseInt(atual.get("espera", "40"))) {
            Screenshot.grab(minecraft.gameDirectory, "leitor_oficial_" + atual.nome() + ".png", minecraft.gameRenderer.mainRenderTarget(), 1,
                  message -> Mekanism.logger.info("cena: {}", message.getString()));
            atual = null;
            pausa = 10;
        }
    }
}
