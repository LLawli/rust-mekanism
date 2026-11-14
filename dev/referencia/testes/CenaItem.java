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

/// Roda o roteiro `cena-item.json` (`dev/referencia/cenas/blocos.json`, o mesmo que o port roda por `horizon2/h2_blocos.py cena`) no cliente do original: os comandos de
/// `setup`, o F1 para esconder a tela, e para cada tiro limpa a fileira, monta os blocos por `setblock`, põe a câmera e tira o print `oficial_<tiro>.png` pelo próprio jogo.
/// Só age com o arquivo `cena-item.json` na pasta do jogo, para não mexer no cliente de uso normal.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaItem {

    private static final Path ATIVA = Path.of("cena-item.json");
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
        // A escala automática do original numa janela de 1920 por 1080 é 4 (o port a recebe por `options-set guiScale 4`); o chat escondido, porque o Horizon não mostra o retorno dos comandos.
        actions.add(new Action(10, () -> {
            minecraft.options.guiScale().set(4);
            minecraft.resizeGui();
            minecraft.options.chatVisibility().set(net.minecraft.world.entity.player.ChatVisiblity.HIDDEN);
        }));
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
        String clear = spec.has("clear") ? spec.get("clear").getAsString() : "weather clear";
        for (JsonElement element : spec.getAsJsonArray("shots")) {
            JsonObject shot = element.getAsJsonObject();
            actions.add(new Action(at, () -> run(server, clear)));
            at += 5;
            JsonArray commands = shot.getAsJsonArray("cmds");
            for (JsonElement command : commands) {
                if (command.getAsString().startsWith("#esperar ")) {
                    // `#esperar N`: N ticks do cliente sem comando (o servidor anda, e o tile publica o estado antes do `tick freeze`).
                    at += Integer.parseInt(command.getAsString().substring(9).trim());
                    continue;
                }
                actions.add(new Action(at++, () -> run(server, command.getAsString())));
            }
            if (shot.has("tp")) {
                actions.add(new Action(at, () -> run(server, "tp @a " + shot.get("tp").getAsString())));
            }
            at += SETTLE;
            String name = shot.get("name").getAsString();
            actions.add(new Action(at, () -> shot(minecraft, "oficial_" + name)));
            at += GAP;
        }
        actions.add(new Action(at + 20, minecraft::stop));
        return actions;
    }

    private static void run(MinecraftServer server, String command) {
        Minecraft minecraft = Minecraft.getInstance();
        if (command.startsWith("#")) {
            String[] words = command.split("\\s+");
            switch (words[0]) {
                case "#inventario" -> inventory(minecraft, Integer.parseInt(words[1]));
                case "#tela" -> minecraft.setScreen(null);
                case "#colocar" -> useOn(minecraft, pos(words), true);
                case "#abrir" -> useOn(minecraft, pos(words), false);
                case "#quebrar" -> server.execute(() -> server.overworld().destroyBlock(pos(words), true, server.getPlayerList().getPlayers().get(0)));
                case "#cursor" -> cursor(minecraft, words[1]);
                case "#clicar" -> click(minecraft, words[1]);
                default -> throw new IllegalArgumentException(command);
            }
            return;
        }
        server.execute(() -> server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command));
    }

    private static net.minecraft.core.BlockPos pos(String[] words) {
        return new net.minecraft.core.BlockPos(Integer.parseInt(words[1]), Integer.parseInt(words[2]), Integer.parseInt(words[3]));
    }

    /// `#colocar X Y Z` (o bloco em `X Y Z` é o chão, a face de cima) e `#abrir X Y Z` (a mão vazia no bloco): o `useItemOn` que o jogador faz com o botão direito.
    private static void useOn(Minecraft minecraft, net.minecraft.core.BlockPos pos, boolean place) {
        var hit = new net.minecraft.world.phys.BlockHitResult(net.minecraft.world.phys.Vec3.atCenterOf(pos).add(0, place ? 0.5 : 0, 0), place ? net.minecraft.core.Direction.UP : net.minecraft.core.Direction.NORTH, pos, false);
        minecraft.gameMode.useItemOn(minecraft.player, net.minecraft.world.InteractionHand.MAIN_HAND, hit);
    }

    /// `#inventario N`: a tela do inventário aberta com o cursor no centro do slot `N` da barra de atalhos (o tooltip do item aparece).
    private static void inventory(Minecraft minecraft, int hotbar) {
        var screen = new net.minecraft.client.gui.screens.inventory.InventoryScreen(minecraft.player);
        minecraft.setScreen(screen);
        try {
            var left = net.minecraft.client.gui.screens.inventory.AbstractContainerScreen.class.getDeclaredField("leftPos");
            var top = net.minecraft.client.gui.screens.inventory.AbstractContainerScreen.class.getDeclaredField("topPos");
            left.setAccessible(true);
            top.setAccessible(true);
            var slot = screen.getMenu().slots.get(36 + hotbar);
            mouse(minecraft, left.getInt(screen) + slot.x + 8.5, top.getInt(screen) + slot.y + 8.5);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    /// O cursor: `MouseHandler` guarda a posição em pixels da janela, e o jogo a escala para a GUI na hora de desenhar.
    private static void mouse(Minecraft minecraft, double guiX, double guiY) throws ReflectiveOperationException {
        double scale = minecraft.getWindow().getGuiScale();
        var x = net.minecraft.client.MouseHandler.class.getDeclaredField("xpos");
        var y = net.minecraft.client.MouseHandler.class.getDeclaredField("ypos");
        x.setAccessible(true);
        y.setAccessible(true);
        x.setDouble(minecraft.mouseHandler, guiX * scale);
        y.setDouble(minecraft.mouseHandler, guiY * scale);
    }

    /// `#cursor X,Y`: pixels de GUI relativos ao canto da tela da máquina.
    private static void cursor(Minecraft minecraft, String spec) {
        if (minecraft.gui.screen() instanceof mekanism.client.gui.IGuiWrapper wrapper) {
            String[] xy = spec.split(",");
            try {
                mouse(minecraft, wrapper.getLeftPos() + Double.parseDouble(xy[0]) + 0.5, wrapper.getTopPos() + Double.parseDouble(xy[1]) + 0.5);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }
    }

    /// `#clicar X,Y`: o clique esquerdo na tela da máquina, em pixels de GUI relativos ao canto dela.
    private static void click(Minecraft minecraft, String spec) {
        if (minecraft.gui.screen() instanceof mekanism.client.gui.IGuiWrapper wrapper) {
            String[] xy = spec.split(",");
            double x = wrapper.getLeftPos() + Double.parseDouble(xy[0]) + 0.5;
            double y = wrapper.getTopPos() + Double.parseDouble(xy[1]) + 0.5;
            var info = new net.minecraft.client.input.MouseButtonInfo(com.mojang.blaze3d.platform.InputConstants.MOUSE_BUTTON_LEFT, 0);
            var screen = minecraft.gui.screen();
            screen.mouseClicked(new net.minecraft.client.input.MouseButtonEvent(x, y, info), false);
            screen.mouseReleased(new net.minecraft.client.input.MouseButtonEvent(x, y, info));
        }
    }

    private static void shot(Minecraft minecraft, String name) {
        // O núcleo do cubo gira com o tempo do jogo (`gameTime + partialTick`): o tempo do print vai no log, e o port o alcança com `tick step` (`h2_blocos.py`).
        Mekanism.logger.info("cena: {} gametime {}", name, minecraft.level.getGameTime());
        Screenshot.grab(minecraft.gameDirectory, name + ".png", minecraft.gameRenderer.mainRenderTarget(), 1, message -> Mekanism.logger.info("cena: {}", message.getString()));
    }
}
