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
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.ChatVisiblity;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;

/// A forma de seleção e a de colisão dos transmissores no cliente oficial (frente `cabo-forma`, Fase 4): a mesma cena de `horizon2/h2_cabo_forma.py` (as coordenadas do arquivo `cena-forma`
/// são relativas à origem 300 -61 10 daqui). Uma ação por linha (`a=<verbo> chave=valor ...`, sem espaços nos valores):
///
///   a=limpa                       limpa a área e põe o chão de pedra
///   a=bloco x y z b               põe o bloco (com o estado padrão)
///   a=dados x y z nbt             `data merge block`
///   a=tipo x y z lado tipo        o tipo cru de um lado do transmissor (o que o Configurator agachado faz)
///   a=espera n                    n tiques
///   a=camera nome x y z ax ay az espera   põe o jogador nos pés (x y z), olhando (ax ay az), e tira o print `forma_oficial_<nome>.png` depois de `espera` tiques de mão vazia e o mesmo quadro sem o contorno do bloco (`forma_oficial_<nome>_sem.png`)
///   a=item nome x y z             solta uma pedra parada em (x y z) com a marca `nome`
///   a=ler                         escreve `cena: item <nome> x y z` de cada item solto
///   a=mata                        mata os itens
///   a=cai nome x y z yaw          o jogador cai em (x y z), 40 tiques, e escreve `cena: pessoa <nome> x y z`
///   a=anda nome x y z yaw n       o jogador em (x y z), com a tecla para frente por n tiques, e escreve `cena: pessoa <nome> x y z`
///   a=fim                         fecha o jogo
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaForma {

    private static final Path ATIVA = Path.of("cena-forma");
    private static final int OX = 300;
    private static final int OY = -61;
    private static final int OZ = 10;

    private record Acao(String verbo, Map<String, String> campos) {
        String get(String chave, String padrao) {
            return campos.getOrDefault(chave, padrao);
        }

        double num(String chave) {
            return Double.parseDouble(campos.get(chave));
        }

        int inteiro(String chave) {
            return (int) Math.round(Double.parseDouble(campos.get(chave)));
        }
    }

    private static final List<Acao> acoes = new ArrayList<>();
    private static boolean loaded;
    private static boolean done;
    private static int ticks;
    private static int indice = -1;
    private static int pausa;
    private static Acao esperando;
    private static int contagem;
    private static int fase;

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
                acoes.add(new Acao(campos.getOrDefault("a", "x"), campos));
            }
        } catch (IOException e) {
            Mekanism.logger.error("cena-forma: leitura", e);
        }
        loaded = true;
    }

    private static void run(MinecraftServer server, String command) {
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command);
    }

    private static String num(double v) {
        return String.format(Locale.ROOT, "%.4f", v);
    }

    private static Direction lado(String nome) {
        return Direction.byName(nome);
    }

    private static void logPessoa(MinecraftServer server, String nome) {
        ServerPlayer player = server.getPlayerList().getPlayers().get(0);
        Mekanism.logger.info("cena: pessoa {} {} {} {} yaw {} pitch {}", nome, player.getX(), player.getY(), player.getZ(), player.getYRot(), player.getXRot());
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
            server.execute(() -> {
                run(server, "difficulty peaceful");
                run(server, "time set 6000");
                run(server, "weather clear");
                run(server, "gamerule advance_time false");
                run(server, "gamerule advance_weather false");
                run(server, "gamerule send_command_feedback false");
                run(server, "gamemode creative @a");
                Mekanism.logger.info("cena: origem {} {} {}", OX, OY, OZ);
            });
        }
        if (ticks < 40) {
            return;
        }
        if (pausa > 0) {
            pausa--;
            return;
        }
        if (esperando != null) {
            contagem--;
            Acao a = esperando;
            if (a.verbo().equals("anda")) {
                if (contagem == 0) {
                    minecraft.options.keyUp.setDown(false);
                    esperando = null;
                    pausa = 10;
                    server.execute(() -> logPessoa(server, a.get("nome", "x")));
                }
            } else if (a.verbo().equals("camera") && contagem == 0) {
                if (fase == 0) {
                    BlockHitResult mira = minecraft.hitResult instanceof BlockHitResult b ? b : null;
                    Mekanism.logger.info("cena: foto {} jogador {} {} {} olhos {} yaw {} pitch {} mira {}", a.get("nome", "x"), minecraft.player.getX(), minecraft.player.getY(), minecraft.player.getZ(),
                          minecraft.player.getEyeY(), minecraft.player.getYRot(), minecraft.player.getXRot(), mira == null ? "nada" : mira.getBlockPos().subtract(new BlockPos(OX, OY, OZ)) + " " + mira.getDirection());
                    Screenshot.grab(minecraft.gameDirectory, "forma_oficial_" + a.get("nome", "x") + ".png", minecraft.gameRenderer.mainRenderTarget(), 1,
                          message -> Mekanism.logger.info("cena: {}", message.getString()));
                    // O mesmo quadro sem o contorno do bloco (`GameRenderer.setRenderBlockOutline`): a diferença entre os dois é só o contorno.
                    minecraft.gameRenderer.setRenderBlockOutline(false);
                    fase = 1;
                    contagem = 6;
                } else {
                    Screenshot.grab(minecraft.gameDirectory, "forma_oficial_" + a.get("nome", "x") + "_sem.png", minecraft.gameRenderer.mainRenderTarget(), 1,
                          message -> Mekanism.logger.info("cena: {}", message.getString()));
                    minecraft.gameRenderer.setRenderBlockOutline(true);
                    fase = 0;
                    esperando = null;
                    pausa = 10;
                }
            } else if (a.verbo().equals("cai") && contagem == 0) {
                esperando = null;
                server.execute(() -> logPessoa(server, a.get("nome", "x")));
            }
            return;
        }
        indice++;
        if (indice >= acoes.size()) {
            done = true;
            Mekanism.logger.info("cena: fim");
            minecraft.stop();
            return;
        }
        Acao a = acoes.get(indice);
        switch (a.verbo()) {
            case "limpa" -> server.execute(() -> {
                run(server, String.format(Locale.ROOT, "fill %d %d %d %d %d %d minecraft:stone", OX - 8, OY - 1, OZ - 8, OX + 12, OY - 1, OZ + 40));
                run(server, String.format(Locale.ROOT, "fill %d %d %d %d %d %d minecraft:air", OX - 8, OY, OZ - 8, OX + 12, OY + 8, OZ + 40));
                run(server, "kill @e[type=item]");
                run(server, "clear @a");
            });
            case "bloco" -> server.execute(() -> run(server, String.format(Locale.ROOT, "setblock %d %d %d %s", OX + a.inteiro("x"), OY + a.inteiro("y"), OZ + a.inteiro("z"), a.get("b", "minecraft:air"))));
            case "dados" -> server.execute(() -> run(server, String.format(Locale.ROOT, "data merge block %d %d %d %s", OX + a.inteiro("x"), OY + a.inteiro("y"), OZ + a.inteiro("z"), a.get("nbt", "{}"))));
            case "tipo" -> server.execute(() -> {
                ServerLevel level = server.overworld();
                if (level.getBlockEntity(new BlockPos(OX + a.inteiro("x"), OY + a.inteiro("y"), OZ + a.inteiro("z"))) instanceof TileEntityTransmitter tile) {
                    Direction side = lado(a.get("lado", "west"));
                    tile.getTransmitter().setConnectionTypeRaw(side, ConnectionType.valueOf(a.get("tipo", "NORMAL")));
                    tile.getTransmitter().onModeChange(side);
                    tile.getTransmitter().refreshConnections();
                    tile.getTransmitter().notifyTileChange();
                    tile.sendUpdatePacket();
                } else {
                    Mekanism.logger.error("cena: sem transmissor em {}", a);
                }
            });
            case "espera" -> pausa = a.inteiro("n");
            case "camera" -> {
                // O `tp ... facing` olha dos pés (a âncora padrão da fonte do comando): a rotação sai da conta, dos olhos (1,62 acima dos pés) ao alvo.
                double fx = OX + a.num("x"), fy = OY + a.num("y"), fz = OZ + a.num("z");
                double dx = OX + a.num("ax") - fx, dy = OY + a.num("ay") - (fy + 1.62), dz = OZ + a.num("az") - fz;
                double yaw = Math.toDegrees(Math.atan2(-dx, dz));
                double pitch = -Math.toDegrees(Math.atan2(dy, Math.sqrt(dx * dx + dz * dz)));
                server.execute(() -> run(server, String.format(Locale.ROOT, "tp @a %s %s %s %s %s", num(fx), num(fy), num(fz), num(yaw), num(pitch))));
                esperando = a;
                contagem = a.inteiro("espera");
            }
            case "item" -> server.execute(() -> run(server, String.format(Locale.ROOT,
                  "summon item %s %s %s {Item:{id:\"minecraft:stone\",count:1},Motion:[0.0,0.0,0.0],PickupDelay:32767s,Tags:[\"forma\",\"%s\"]}",
                  num(OX + a.num("x")), num(OY + a.num("y")), num(OZ + a.num("z")), a.get("nome", "x").replace('.', '_'))));
            case "ler" -> server.execute(() -> {
                for (ItemEntity item : server.overworld().getEntitiesOfClass(ItemEntity.class, new AABB(OX - 20, OY - 5, OZ - 20, OX + 40, OY + 20, OZ + 60))) {
                    String nome = item.entityTags().stream().filter(t -> !t.equals("forma")).findFirst().orElse("?");
                    Mekanism.logger.info("cena: item {} {} {} {}", nome, item.getX(), item.getY(), item.getZ());
                }
            });
            case "mata" -> server.execute(() -> run(server, "kill @e[type=item]"));
            case "cai" -> {
                server.execute(() -> run(server, String.format(Locale.ROOT, "tp @a %s %s %s %s 0", num(OX + a.num("x")), num(OY + a.num("y")), num(OZ + a.num("z")), num(a.num("yaw")))));
                esperando = a;
                contagem = 50;
            }
            case "anda" -> {
                server.execute(() -> run(server, String.format(Locale.ROOT, "tp @a %s %s %s %s 0", num(OX + a.num("x")), num(OY + a.num("y")), num(OZ + a.num("z")), num(a.num("yaw")))));
                esperando = a;
                contagem = a.inteiro("n") + 10;
                pausa = 10;
                minecraft.options.keyUp.setDown(true);
            }
            case "fim" -> {
                done = true;
                Mekanism.logger.info("cena: fim");
                minecraft.stop();
            }
            default -> Mekanism.logger.error("cena: verbo desconhecido {}", a);
        }
    }
}
