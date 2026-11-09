package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.common.Mekanism;
import mekanism.common.tests.MekanismTests;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.server.MinecraftServer;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;

/// Roda o roteiro `cena-blocos.json` (`dev/referencia/cenas/blocos.json`, o mesmo que o port roda por `horizon2/h2_blocos.py cena`) no cliente do original: os comandos de
/// `setup`, o F1 para esconder a tela, e para cada tiro limpa a fileira, monta os blocos por `setblock`, põe a câmera e tira o print `oficial_<tiro>.png` pelo próprio jogo.
/// Só age com o arquivo `cena-blocos.json` na pasta do jogo, para não mexer no cliente de uso normal.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaBlocos {

    private static final Path ATIVA = Path.of("cena-blocos.json");
    private static final int SETTLE = 60;
    private static final int GAP = 20;

    private record Action(int tick, Runnable run) {
    }

    private static List<Action> plan;
    private static int ticks;

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (minecraft.level == null || minecraft.player == null || server == null || !Files.exists(ATIVA)) {
            return;
        }
        if (plan == null) {
            plan = build(minecraft, server);
        }
        ticks++;
        for (Action action : plan) {
            if (action.tick == ticks) {
                action.run.run();
            }
        }
    }

    private static List<Action> build(Minecraft minecraft, MinecraftServer server) {
        JsonObject spec;
        try {
            spec = JsonParser.parseString(Files.readString(ATIVA)).getAsJsonObject();
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
        List<Action> actions = new ArrayList<>();
        int at = 20;
        for (JsonElement command : spec.getAsJsonArray("setup")) {
            actions.add(new Action(at++, () -> run(server, command.getAsString())));
        }
        at += 30;
        // `"hud": true` no roteiro deixa a tela ligada (a mão com o item e a barra de atalhos aparecem; o F1 esconde a mão).
        if (!spec.has("hud") || !spec.get("hud").getAsBoolean()) {
            actions.add(new Action(at, () -> KeyMapping.click(minecraft.options.keyToggleGui.getKey())));
        }
        at += 30;
        String clear = spec.get("clear").getAsString();
        for (JsonElement element : spec.getAsJsonArray("shots")) {
            JsonObject shot = element.getAsJsonObject();
            actions.add(new Action(at, () -> run(server, clear)));
            at += 5;
            JsonArray commands = shot.getAsJsonArray("cmds");
            for (JsonElement command : commands) {
                actions.add(new Action(at++, () -> run(server, command.getAsString())));
            }
            actions.add(new Action(at, () -> run(server, "tp @a " + shot.get("tp").getAsString())));
            at += SETTLE;
            String name = shot.get("name").getAsString();
            actions.add(new Action(at, () -> shot(minecraft, "oficial_" + name)));
            at += GAP;
        }
        actions.add(new Action(at + 20, minecraft::stop));
        return actions;
    }

    private static void run(MinecraftServer server, String command) {
        server.execute(() -> server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command));
    }

    private static void shot(Minecraft minecraft, String name) {
        Screenshot.grab(minecraft.gameDirectory, name + ".png", minecraft.gameRenderer.mainRenderTarget(), 1, message -> Mekanism.logger.info("cena: {}", message.getString()));
    }
}
