package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import mekanism.client.gui.GuiRadialSelector;
import mekanism.client.key.MekanismKeyHandler;
import mekanism.client.render.hud.MekanismStatusOverlay;
import mekanism.common.Mekanism;
import mekanism.common.item.ItemConfigurator;
import mekanism.common.registries.MekanismItems;
import mekanism.common.tests.MekanismTests;
import net.minecraft.client.CameraType;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.server.MinecraftServer;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.player.ChatVisiblity;
import net.minecraft.world.item.ItemStack;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;

/// Os efeitos de tela do Configurator no cliente oficial (decisão 48 do plano): a sobreposição da face, o texto do modo sobre a barra, o texto do HUD e o menu radial, cada um num
/// print de 1920 por 1080 (`guiScale` 4: 480 por 270 pixels de GUI). Só age com o arquivo `cena-configurator` ao lado, uma etapa por linha (`chave=valor` separados por espaço):
///
///   nome=overlay_items_sul efeito=overlay modo=configurate_items tp=300.5,-58.38,3.5,180,24 config=1,4,7,3,5,9
///   nome=status_cheio efeito=status modo=wrench espera=4 sobrevivencia=0
///   nome=hud efeito=hud modo=rotate
///   nome=radial_centro efeito=radial modo=configurate_fluids mouse=0,0
///   nome=div_com_sinal efeito=desviador modo=rotate modos=0,0,0,1,0,0 tp=300.5,-61,2.6,180,31
///
/// Os campos: `nome` (o print sai em `screenshots/configurator_oficial_<nome>.png`), `efeito`, `modo` (o `ConfiguratorMode` do item na mão, no slot 0 da barra), `tp` (`x,y,z,yaw,pitch` do
/// jogador), `config` (para `overlay`: os `DataType` por `RelativeSide` do tipo itens da câmara de enriquecimento posta em (300, -60, 0) virada para o norte, e energia fica fixa),
/// `espera` (tiques depois de montar a cena até o print; padrão 60), `sobrevivencia` (`1`: a etapa roda em sobrevivência), `mouse` (`dx,dy` em pixels de GUI a partir do centro, para
/// `radial`; o cursor do menu é posto pelo `MouseHandler`).
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaConfigurator {

    private static final Path ATIVA = Path.of("cena-configurator");
    private static final int X = 300;
    private static final int Y = -61;
    private static final int Z = 0;
    private static final int ENTRADA = 10;
    private static final int PAUSA = 10;
    private static final int GUI_SCALE = 4;

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
    private static boolean radial;
    private static String[] mouseAtual = {"0", "0"};

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
            Mekanism.logger.error("cena-configurator: leitura", e);
        }
        loaded = true;
    }

    private static void run(MinecraftServer server, String command) {
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command);
    }

    /// `MouseHandler.xpos` e `ypos` são em pixels da janela: o cursor do menu fica a `dx, dy` pixels de GUI do centro.
    private static void mouse(Minecraft minecraft, double dx, double dy) {
        try {
            Field x = minecraft.mouseHandler.getClass().getDeclaredField("xpos");
            Field y = minecraft.mouseHandler.getClass().getDeclaredField("ypos");
            x.setAccessible(true);
            y.setAccessible(true);
            x.setDouble(minecraft.mouseHandler, minecraft.getWindow().getScreenWidth() / 2.0 + dx * GUI_SCALE);
            y.setDouble(minecraft.mouseHandler, minecraft.getWindow().getScreenHeight() / 2.0 + dy * GUI_SCALE);
        } catch (ReflectiveOperationException e) {
            Mekanism.logger.error("cena-configurator: mouse", e);
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
            // O original mostra o aviso de alfa do Mekanism e a conquista "Configure Everything" (o port não tem as duas): o chat fica escondido e os avisos saem a cada tique.
            minecraft.options.chatVisibility().set(ChatVisiblity.HIDDEN);
        }
        minecraft.gui.toastManager().clear();
        if (ticks == 20) {
            server.execute(() -> run(server, "difficulty peaceful"));
            server.execute(() -> run(server, "time set 6000"));
            server.execute(() -> run(server, "weather clear"));
            server.execute(() -> run(server, "gamerule send_command_feedback false"));
            server.execute(() -> run(server, "fill 292 -61 -8 308 -40 12 minecraft:air"));
            server.execute(() -> run(server, "fill 292 -62 -8 308 -62 12 minecraft:grass_block"));
            server.execute(() -> run(server, "fill 292 -61 -3 308 -50 -3 minecraft:black_concrete"));
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
                run(server, "gamemode " + (e.get("sobrevivencia", "0").equals("1") ? "survival" : "creative") + " @a");
                run(server, "clear @a");
                // `item=` põe outro item na mão (um graveto: sem Configurator não há sobreposição, e o print mostra só o desviador, o controle da comparação).
                run(server, "item replace entity @a hotbar.0 with " + (e.campos().containsKey("item") ? e.get("item", "") + " 1" : "mekanism:configurator[mekanism:configurator_mode=\"" + e.get("modo", "configurate_items") + "\"] 1"));
                run(server, "fill 296 -61 -2 304 -55 2 minecraft:air");
                if (e.get("efeito", "").equals("overlay")) {
                    run(server, String.format(Locale.ROOT, "setblock %d %d %d mekanism:enrichment_chamber[facing=north]", X, Y, Z));
                    // Os `DataType` do tipo itens (`config3`, um por `RelativeSide`) e a energia (`config0`) da câmara, o que o `data merge` do tile lê em `loadAdditional`.
                    run(server, String.format(Locale.ROOT, "data merge block %d %d %d {component_config:{config3:[I;%s],config0:[I;0,0,0,0,0,0]}}", X, Y, Z, e.get("config", "1,4,7,3,5,9")));
                }
                if (e.get("efeito", "").equals("desviador")) {
                    // O desviador isolado em (300, -61, 0), com o `mode` de cada lado (`DiversionControl.ordinal` por `Direction`: baixo, cima, norte, sul, oeste, leste), que o `RenderTickHandler`
                    // lê em `modes[face.ordinal()]`; o configurador na mão em qualquer modo.
                    // O chão do jogo (a grama) sorteia a variante da textura de outro jeito em cada jogo: o desviador é vazado e o chão aparece por trás da sobreposição, então um chão liso
                    // (concreto branco) deixa o fundo igual nos dois e a comparação de pixels mede só a sobreposição.
                    run(server, "fill 296 -62 -4 304 -62 4 minecraft:white_concrete");
                    run(server, String.format(Locale.ROOT, "setblock %d %d %d mekanism:diversion_transporter{mode:[I;%s]}", X, Y, Z, e.get("modos", "0,0,0,0,0,0")));
                }
                String[] tp = e.get("tp", "300.5,-60,3.5,180,10").split(",");
                run(server, String.format(Locale.ROOT, "tp @a %s %s %s %s %s", tp[0], tp[1], tp[2], tp[3], tp[4]));
            });
            return;
        }
        n++;
        if (n == ENTRADA && atual.get("efeito", "").equals("status")) {
            MekanismStatusOverlay.INSTANCE.setTimer();
        }
        if (n == ENTRADA && atual.get("efeito", "").equals("radial")) {
            radial = true;
            mouseAtual = atual.get("mouse", "0,0").split(",");
            reabrir(minecraft);
        }
        if (n == Integer.parseInt(atual.get("espera", "60"))) {
            Screenshot.grab(minecraft.gameDirectory, "configurator_oficial_" + atual.nome() + ".png", minecraft.gameRenderer.mainRenderTarget(), 1,
                  message -> Mekanism.logger.info("cena: {}", message.getString()));
            radial = false;
            if (minecraft.gui.screen() != null) {
                minecraft.gui.setScreen(null);
            }
            atual = null;
            pausa = PAUSA;
        }
    }

    /// `ClientTickHandler` fecha o menu em todo tique em que a tecla do modo não está apertada (`MekKeyHandler.isRadialPressed` lê a tecla física, que a cena não tem): a cena o abre de
    /// novo no fim de cada tique (`LOWEST`, depois do `ClientTickHandler`), então todo quadro desenhado tem o menu, como com a tecla apertada.
    private static void reabrir(Minecraft minecraft) {
        ItemStack stack = minecraft.player.getMainHandItem();
        if (stack.getItem() instanceof ItemConfigurator item && !(minecraft.gui.screen() instanceof GuiRadialSelector)) {
            GuiRadialSelector selector = new GuiRadialSelector(EquipmentSlot.MAINHAND, item.getRadialData(stack), () -> minecraft.player);
            // Fechado e aberto a cada tique, o menu mandaria o modo sob o mouse (`removed` -> `updateSelection`) a cada vez: com a tecla apertada ele ficaria aberto e nada iria.
            try {
                Field updateOnClose = GuiRadialSelector.class.getDeclaredField("updateOnClose");
                updateOnClose.setAccessible(true);
                updateOnClose.setBoolean(selector, false);
            } catch (ReflectiveOperationException e) {
                Mekanism.logger.error("cena-configurator: updateOnClose", e);
            }
            minecraft.gui.setScreen(selector);
        }
        mouse(minecraft, Double.parseDouble(mouseAtual[0]), Double.parseDouble(mouseAtual[1]));
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    public static void onTickDepois(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        if (radial && minecraft.player != null) {
            reabrir(minecraft);
        }
    }
}
