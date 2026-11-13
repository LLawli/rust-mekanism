package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import mekanism.common.Mekanism;
import mekanism.common.lib.transmitter.ConnectionType;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tile.transmitter.TileEntityTransmitter;
import net.minecraft.client.CameraType;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.ChatVisiblity;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.HitResult;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;

/// A mira nos transmissores no cliente oficial (frente `cliente-mira`, decisões 166 a 168 do plano): o contorno do Configurator num braço `NORMAL`, `PUSH` e `PULL` de um cabo, e o
/// clique da liga no centro de um cabo (a liga de verdade, `ItemAlloy.useOn`, pelo `MultiPlayerGameMode.useItemOn` do que o jogo mira). Só age com o arquivo `cena-mira` ao lado, uma
/// etapa por linha (`chave=valor` separados por espaço):
///
///   nome=contorno_push efeito=contorno tipo=PUSH
///   nome=liga_centro efeito=liga item=mekanism:alloy_infused
///
/// Os campos: `nome` (o print sai em `screenshots/mira_oficial_<nome>.png`), `efeito` (`contorno`: o Configurator em `configurate_items` na mão, o braço leste do cabo do meio de uma
/// linha de dois cabos que dá num cubo de energia criativo, no tipo `tipo`, com a câmera a 2,6 blocos ao norte, na mesma posição relativa do port: yaw -6,4416 e pitch 19,7486;
/// `liga`: uma linha de três cabos, a câmera olhando o centro do cabo do meio, e o clique com o `item` na mão), `tipo` (`NORMAL`, `PUSH` ou `PULL`), `item` (o item da mão, na
/// etapa `liga`) e `espera` (tiques depois de montar até o print; padrão 60).
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaMira {

    private static final Path ATIVA = Path.of("cena-mira");
    private static final int X = 300;
    private static final int Y = -61;
    private static final int Z = 10;
    private static final int ENTRADA = 20;
    private static final int PAUSA = 10;
    private static final String CABO = "mekanism:basic_universal_cable";

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
                Map<String, String> campos = new HashMap<>();
                for (String word : line.split("\\s+")) {
                    String[] kv = word.split("=", 2);
                    if (kv.length == 2) {
                        campos.put(kv[0], kv[1]);
                    }
                }
                etapas.add(new Etapa(campos.getOrDefault("nome", "x"), campos));
            }
        } catch (IOException e) {
            Mekanism.logger.error("cena-mira: leitura", e);
        }
        loaded = true;
    }

    private static void run(MinecraftServer server, String command) {
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command);
    }

    /// O tipo cru do braço leste do cabo do meio: o que o Configurator agachado faz (`setConnectionTypeRaw`, `onModeChange`, `refreshConnections`, `notifyTileChange` e o pacote).
    private static void tipo(MinecraftServer server, String nome) {
        ServerLevel level = server.overworld();
        if (level.getBlockEntity(new BlockPos(X + 1, Y, Z)) instanceof TileEntityTransmitter tile) {
            tile.getTransmitter().setConnectionTypeRaw(Direction.EAST, ConnectionType.valueOf(nome));
            tile.getTransmitter().onModeChange(Direction.EAST);
            tile.getTransmitter().refreshConnections();
            tile.getTransmitter().notifyTileChange();
            tile.sendUpdatePacket();
        } else {
            Mekanism.logger.error("cena: sem o cabo em {} {} {}", X + 1, Y, Z);
        }
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
            minecraft.options.chatVisibility().set(ChatVisiblity.HIDDEN);
        }
        minecraft.gui.toastManager().clear();
        if (ticks == 20) {
            server.execute(() -> run(server, "difficulty peaceful"));
            server.execute(() -> run(server, "time set 6000"));
            server.execute(() -> run(server, "weather clear"));
            server.execute(() -> run(server, "gamerule advance_time false"));
            server.execute(() -> run(server, "gamerule advance_weather false"));
            server.execute(() -> run(server, "gamerule send_command_feedback false"));
            server.execute(() -> run(server, "fill 292 -61 2 308 -40 22 minecraft:air"));
            server.execute(() -> run(server, "fill 292 -62 2 308 -62 22 minecraft:stone"));
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
                run(server, "gamemode creative @a");
                run(server, "clear @a");
                if (e.get("efeito", "").equals("contorno")) {
                    run(server, "item replace entity @a hotbar.0 with mekanism:configurator[mekanism:configurator_mode=\"configurate_items\"] 1");
                    run(server, "fill 296 -61 8 308 -55 20 minecraft:air");
                    run(server, String.format(Locale.ROOT, "setblock %d %d %d %s", X, Y, Z, CABO));
                    run(server, String.format(Locale.ROOT, "setblock %d %d %d %s", X + 1, Y, Z, CABO));
                    run(server, String.format(Locale.ROOT, "setblock %d %d %d mekanism:creative_energy_cube", X + 2, Y, Z));
                    run(server, String.format(Locale.ROOT, "data merge block %d %d %d {component_config:{config0:[I;4,4,4,4,4,4],eject0:0b},energy_amount:9223372036854775807L}", X + 2, Y, Z));
                } else {
                    run(server, "item replace entity @a hotbar.0 with " + e.get("item", "mekanism:alloy_infused") + " 1");
                    run(server, "fill 296 -61 8 308 -55 20 minecraft:air");
                    for (int dx = 0; dx < 3; dx++) {
                        run(server, String.format(Locale.ROOT, "setblock %d %d %d %s", X + dx, Y, Z + 4, CABO));
                    }
                }
            });
            return;
        }
        n++;
        if (n == ENTRADA) {
            server.execute(() -> {
                if (atual.get("efeito", "").equals("contorno")) {
                    tipo(server, atual.get("tipo", "NORMAL"));
                    run(server, String.format(Locale.ROOT, "tp @a %s %d %s %s %s", X + 1.5, Y, Z - 2.6, "-6.4416", "19.7486"));
                } else {
                    // A câmera a 2,6 ao norte do cabo do meio, olhando o centro dele (os olhos a 1,62: pitch de atan(1,12 / 3,1) = 19,8 graus).
                    run(server, String.format(Locale.ROOT, "tp @a %s %d %s %s %s", X + 1.5, Y, Z + 4 - 2.6, "0", "19.8"));
                }
            });
        }
        if (n == Integer.parseInt(atual.get("espera", "60")) - 10 && atual.get("efeito", "").equals("liga")) {
            HitResult hit = minecraft.hitResult;
            if (hit instanceof BlockHitResult block && minecraft.gameMode != null) {
                Mekanism.logger.info("cena: liga mira bloco {} face {} ponto {} {} {}", block.getBlockPos(), block.getDirection(), block.getLocation().x, block.getLocation().y, block.getLocation().z);
                InteractionResult result = minecraft.gameMode.useItemOn(minecraft.player, InteractionHand.MAIN_HAND, block);
                Mekanism.logger.info("cena: liga clique {}", result);
            } else {
                Mekanism.logger.info("cena: liga sem bloco na mira: {}", hit == null ? "nulo" : hit.getType());
            }
        }
        if (n == Integer.parseInt(atual.get("espera", "60"))) {
            Mekanism.logger.info("cena: jogador {} {} {} olhos {} yaw {} pitch {} blocos {} {} {}", minecraft.player.getX(), minecraft.player.getY(), minecraft.player.getZ(), minecraft.player.getEyeY(),
                  minecraft.player.getYRot(), minecraft.player.getXRot(), server.overworld().getBlockState(new BlockPos(X, Y, Z)).getBlock(), server.overworld().getBlockState(new BlockPos(X + 1, Y, Z)).getBlock(),
                  server.overworld().getBlockState(new BlockPos(X + 2, Y, Z)).getBlock());
            if (atual.get("efeito", "").equals("liga")) {
                Mekanism.logger.info("cena: liga bloco depois {}", server.overworld().getBlockState(new BlockPos(X + 1, Y, Z + 4)).getBlock());
            } else {
                Mekanism.logger.info("cena: contorno mira {}", minecraft.hitResult == null ? "nulo" : minecraft.hitResult.getType() + " " + (minecraft.hitResult instanceof BlockHitResult b ? b.getBlockPos() + " " + b.getDirection() : ""));
            }
            Screenshot.grab(minecraft.gameDirectory, "mira_oficial_" + atual.nome() + ".png", minecraft.gameRenderer.mainRenderTarget(), 1,
                  message -> Mekanism.logger.info("cena: {}", message.getString()));
            atual = null;
            pausa = PAUSA;
        }
    }
}
