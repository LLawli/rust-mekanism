package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import mekanism.common.Mekanism;
import mekanism.common.tests.MekanismTests;
import net.minecraft.client.CameraType;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.server.MinecraftServer;
import net.minecraft.sounds.SoundSource;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;

/// A armadura vestida no cliente oficial: o jogador de pé no chão, de frente para a câmera (`CameraType.THIRD_PERSON_FRONT`), com as peças da etapa nos slots de armadura,
/// e um print. Só age com o arquivo `cena-equip` ao lado, uma linha por etapa:
///
///   vestir nome=mekasuit pecas=head:mekasuit_helmet,chest:mekasuit_bodyarmor,legs:mekasuit_pants,feet:mekasuit_boots
///
/// `pecas` são pares `slot:item` (os ids do `mekanism:`; um id com `:` é lido inteiro). Os prints saem em `screenshots/equip_oficial_<nome>.png`.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaEquip {

    private static final Path ATIVA = Path.of("cena-equip");
    private static final double X = 300.5;
    private static final double Y = -60;
    private static final double Z = 3.5;
    private static final int ESPERA = 40;
    private static final int PAUSA = 10;

    private record Etapa(String nome, List<String[]> pecas) {
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
                String nome = "x";
                List<String[]> pecas = new ArrayList<>();
                for (String word : line.split("\\s+")) {
                    String[] kv = word.split("=", 2);
                    if (kv.length < 2) {
                        continue;
                    }
                    if (kv[0].equals("nome")) {
                        nome = kv[1];
                    } else if (kv[0].equals("pecas")) {
                        for (String peca : kv[1].split(",")) {
                            pecas.add(peca.split(":", 2));
                        }
                    }
                }
                etapas.add(new Etapa(nome, pecas));
            }
        } catch (IOException e) {
            Mekanism.logger.error("cena-equip: leitura", e);
        }
        loaded = true;
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
        }
        if (ticks == 20) {
            server.execute(() -> run(server, "difficulty peaceful"));
            server.execute(() -> run(server, "time set 6000"));
            server.execute(() -> run(server, "weather clear"));
            server.execute(() -> run(server, "gamerule send_command_feedback false"));
            server.execute(() -> run(server, "gamemode creative @a"));
            server.execute(() -> run(server, "fill 292 -61 -5 308 -40 12 minecraft:air"));
            server.execute(() -> run(server, "fill 292 -62 -5 308 -62 12 minecraft:grass_block"));
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
                run(server, "clear @a");
                run(server, String.format(Locale.ROOT, "tp @a %.2f %.2f %.2f 0 10", X, Y, Z));
                for (String[] peca : e.pecas()) {
                    run(server, "item replace entity @a armor." + peca[0] + " with mekanism:" + peca[1] + " 1");
                }
            });
            minecraft.options.setCameraType(CameraType.THIRD_PERSON_FRONT);
            return;
        }
        n++;
        if (n == ESPERA) {
            Screenshot.grab(minecraft.gameDirectory, "equip_oficial_" + atual.nome() + ".png", minecraft.gameRenderer.mainRenderTarget(), 1,
                  message -> Mekanism.logger.info("cena: {}", message.getString()));
            atual = null;
            pausa = PAUSA;
        }
    }
}
