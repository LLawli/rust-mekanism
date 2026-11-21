package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.common.Mekanism;
import mekanism.common.capabilities.energy.BasicEnergyContainer;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.client.Minecraft;
import net.minecraft.client.MouseHandler;
import net.minecraft.client.Screenshot;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;

/// Dois clientes do original ao mesmo tempo (decisão 14 do plano): o ANFITRIÃO é o `runGameTestClient` de sempre (mundo local aberto à LAN) e o CONVIDADO é outro
/// processo do mesmo cliente, com outra pasta de jogo e outro nome, que entra na LAN. Os dois rodam esta classe; o papel vem do arquivo na pasta do jogo:
/// `cena-dois` (anfitrião, o maestro) ou `cena-convidado` (convidado, só obedece). O maestro escreve `dois-sync/comandos.txt` (uma linha por comando do
/// convidado, `ID acao args`) e espera a linha de `dois-sync/respostas.jsonl` com o mesmo ID. Tudo o que o convidado mostra ou faz sai nessas respostas, no log
/// (`dois:`) e nos prints da pasta dele.
///
/// Roteiro do anfitrião (`cena-dois`, uma ação por linha, `#` comenta):
///   lan PORTA                     abre o mundo à LAN (`publishServer`) na porta
///   espera_convidado              espera o segundo jogador entrar
///   srv COMANDO...                comando do servidor (o `performPrefixedCommand` do `CenaTelas`)
///   caixa X                       a caixa de concreto preto da `CenaTelas` em volta de (X, -60, 3)
///   montar MAQUINA X             o bloco da máquina em (X, -60, 3) como um jogador o põe (`CenaTelas.build`: `applyComponents`)
///   estado X k=v...              o estado de `CenaTelas.state` (energia, entrada, saida, quimico, lava, upgrades), depois de um `passos 1`
///   silencio X N                 o upgrade de silêncio no tile de X na contagem N (`CenaSomMedida.setMuffling`)
///   liga X | desliga X           o item de entrada e a energia cheia (`CenaSomMedida.energia`) | a energia a zero
///   anf TECLA ...                 `abrir`, `fechar`, `foto NOME`, `ler NOME`, `observar N NOME` no cliente do anfitrião
///   conv ACAO ...                 o mesmo no convidado (espera a resposta)
///   passos N                      `tick step N` e espera os ticks
///   congela | descongela          `tick freeze` | `tick unfreeze`
///   espera N                      N ticks do cliente
///   fim                           fecha o jogo
///
/// Ações de cliente (nas duas pontas): `abrir X Y Z` (o `useItemOn` do clique direito), `fechar`, `foto NOME` (`oficial_dois_NOME.png`), `clicar X Y [d]` (pixels de GUI
/// relativos à tela; `d` é o botão direito), `cursor X Y`, `escala N`, `ler NOME` (o estado do cliente em uma linha JSON no log e na resposta), `observar N NOME` (uma
/// linha `ler` por tick durante N ticks, e com `X Y Z` o tile do mundo do cliente em vez do da tela), `slot N BOTAO` (`handleContainerInput`), `tipos` (os tipos
/// de upgrade do registro que o cliente tem), `tile NOME X Y Z` (o tile do mundo do cliente: aceso, fator de volume, silenciado), `digitar TEXTO` (um `charTyped` por
/// caractere), `tecla NOME` (`keyPressed`: enter, tab, backspace, escape, space, e), `roda X Y N` (a roda do mouse no ponto) e `painel L A` (só o port o usa).
///
/// Do anfitrião, para a Security Desk (frente `mesa-seguranca`): `por JOGADOR ITEM X` (o jogador põe o bloco e fica o dono) e `freq NOME` (a tabela das frequências de
/// segurança na memória e no disco).
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaDois {

    private static final Path ANFITRIAO = Path.of("cena-dois");
    private static final Path CONVIDADO = Path.of("cena-convidado");
    private static final Path SYNC = Path.of("/home/luka/.local/share/jogo-isolado/mekanism-ref/dois-sync");
    private static final Path COMANDOS = SYNC.resolve("comandos.txt");
    private static final Path RESPOSTAS = SYNC.resolve("respostas.jsonl");
    private static final int Y = -60;
    private static final int Z = 3;
    private static final int WARMUP = 80;

    private static List<String[]> roteiro;
    private static int linha;
    private static int espera;
    private static int ticks;
    private static int proximoId = 1;
    private static int esperandoResposta = -1;
    private static int observando;
    private static String observandoNome = "";
    private static int[] observandoTile;
    private static int vistos;
    private static int tickConvidado;

    private static String papel() {
        if (Files.exists(ANFITRIAO)) {
            return "anfitriao";
        }
        if (Files.exists(CONVIDADO)) {
            return "convidado";
        }
        return null;
    }

    private static void log(String texto) {
        Mekanism.logger.info("dois: {}", texto);
    }

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        String papel = papel();
        if (papel == null || minecraft.level == null || minecraft.player == null) {
            return;
        }
        ticks++;
        if (observando > 0) {
            observando--;
            String nome = observandoNome + "_t" + (tickConvidado++);
            if (observandoTile != null) {
                tile(minecraft, nome, observandoTile[0], observandoTile[1], observandoTile[2]);
            } else {
                ler(minecraft, nome);
            }
        }
        if (papel.equals("convidado")) {
            convidado(minecraft);
        } else {
            anfitriao(minecraft);
        }
    }

    // O convidado -----------------------------------------------------------------------------------------------------------------------------------------------

    private static void convidado(Minecraft minecraft) {
        if (ticks == 1) {
            log("convidado no mundo " + minecraft.player.getName().getString());
        }
        try {
            if (!Files.exists(COMANDOS)) {
                return;
            }
            List<String> todas = Files.readAllLines(COMANDOS);
            while (vistos < todas.size()) {
                String texto = todas.get(vistos++).trim();
                if (texto.isEmpty() || texto.startsWith("#")) {
                    continue;
                }
                String[] palavras = texto.split("\\s+");
                int id = Integer.parseInt(palavras[0]);
                String resposta = acao(minecraft, java.util.Arrays.copyOfRange(palavras, 1, palavras.length));
                Files.writeString(RESPOSTAS, "{\"id\":" + id + ",\"gt\":" + minecraft.level.getGameTime() + ",\"r\":" + resposta + "}\n", StandardOpenOption.CREATE,
                      StandardOpenOption.APPEND);
            }
        } catch (IOException | RuntimeException e) {
            Mekanism.logger.error("dois: convidado", e);
        }
    }

    /// A ação de cliente (nas duas pontas). Devolve um valor JSON.
    private static String acao(Minecraft minecraft, String[] a) {
        switch (a[0]) {
            case "abrir" -> {
                BlockPos pos = new BlockPos(Integer.parseInt(a[1]), Integer.parseInt(a[2]), Integer.parseInt(a[3]));
                BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(pos), Direction.NORTH, pos, false);
                var result = minecraft.gameMode.useItemOn(minecraft.player, InteractionHand.MAIN_HAND, hit);
                log("abrir " + pos + " -> " + result);
                return "\"" + result + "\"";
            }
            case "usar" -> {
                // Um clique direito só, como o `abrir` (o do port repete até a tela abrir).
                BlockPos pos = new BlockPos(Integer.parseInt(a[1]), Integer.parseInt(a[2]), Integer.parseInt(a[3]));
                BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(pos), Direction.NORTH, pos, false);
                var result = minecraft.gameMode.useItemOn(minecraft.player, InteractionHand.MAIN_HAND, hit);
                log("usar " + pos + " -> " + result);
                return "\"" + result + "\"";
            }
            case "fechar" -> {
                minecraft.player.closeContainer();
                return "true";
            }
            case "agachar" -> {
                // A tecla de agachar do cliente: o jogo manda o estado ao servidor no tick seguinte (`ServerboundPlayerInputPacket`).
                minecraft.options.keyShift.setDown(a[1].equals("1"));
                return "true";
            }
            case "foto" -> {
                Screenshot.grab(minecraft.gameDirectory, "oficial_dois_" + a[1] + ".png", minecraft.gameRenderer.mainRenderTarget(), 1,
                      message -> Mekanism.logger.info("dois: {}", message.getString()));
                return "true";
            }
            case "escala" -> {
                minecraft.options.guiScale().set(Integer.parseInt(a[1]));
                minecraft.resizeGui();
                return "true";
            }
            case "clicar" -> {
                if (!(minecraft.gui.screen() instanceof mekanism.client.gui.IGuiWrapper wrapper)) {
                    return "\"sem tela\"";
                }
                double x = wrapper.getLeftPos() + Double.parseDouble(a[1]) + 0.5;
                double y = wrapper.getTopPos() + Double.parseDouble(a[2]) + 0.5;
                int button = a.length > 3 && a[3].equals("d") ? com.mojang.blaze3d.platform.InputConstants.MOUSE_BUTTON_RIGHT
                      : com.mojang.blaze3d.platform.InputConstants.MOUSE_BUTTON_LEFT;
                var info = new net.minecraft.client.input.MouseButtonInfo(button, 0);
                var screen = minecraft.gui.screen();
                screen.mouseClicked(new net.minecraft.client.input.MouseButtonEvent(x, y, info), false);
                screen.mouseReleased(new net.minecraft.client.input.MouseButtonEvent(x, y, info));
                return "true";
            }
            case "digitar" -> {
                // O texto pelo teclado: um `charTyped` da tela por caractere (o que o `KeyboardHandler` entrega).
                var screen = minecraft.gui.screen();
                if (screen == null) {
                    return "\"sem tela\"";
                }
                a[1].codePoints().forEach(codepoint -> screen.charTyped(new net.minecraft.client.input.CharacterEvent(codepoint)));
                return "true";
            }
            case "tecla" -> {
                // Uma tecla apertada na tela (`keyPressed`), pelo nome ou pelo scancode.
                var screen = minecraft.gui.screen();
                if (screen == null) {
                    return "\"sem tela\"";
                }
                // 26.3: a tecla é o scancode do SDL (`InputWithModifiers.isConfirmation`: 40 e 88) e o `keycode` o código da tecla (o Tab é o 9 em `Screen.keyPressed`).
                int[] key = switch (a[1]) {
                    case "enter" -> new int[] {40, 13};
                    case "tab" -> new int[] {43, 9};
                    case "backspace" -> new int[] {42, 8};
                    case "escape" -> new int[] {41, 27};
                    case "space" -> new int[] {44, 32};
                    case "e" -> new int[] {8, 101};
                    default -> new int[] {Integer.parseInt(a[1]), 0};
                };
                boolean handled = screen.keyPressed(new net.minecraft.client.input.KeyEvent(key[0], key[1], 0));
                return handled ? "true" : "false";
            }
            case "roda" -> {
                // A roda do mouse num ponto da tela (pixels de GUI relativos a ela), `N` passos (negativo desce).
                if (!(minecraft.gui.screen() instanceof mekanism.client.gui.IGuiWrapper wrapper)) {
                    return "\"sem tela\"";
                }
                double x = wrapper.getLeftPos() + Double.parseDouble(a[1]) + 0.5;
                double y = wrapper.getTopPos() + Double.parseDouble(a[2]) + 0.5;
                minecraft.gui.screen().mouseScrolled(x, y, 0, Double.parseDouble(a[3]));
                return "true";
            }
            case "painel" -> {
                // Só o port precisa do tamanho da tela (o original o lê da própria tela).
                return "true";
            }
            case "cursor" -> {
                cursor(minecraft, a.length > 1 ? Double.parseDouble(a[1]) : 2, a.length > 2 ? Double.parseDouble(a[2]) : 2);
                return "true";
            }
            case "ler" -> {
                return ler(minecraft, a.length > 1 ? a[1] : "");
            }
            case "observar" -> {
                observando = Integer.parseInt(a[1]);
                observandoNome = a.length > 2 ? a[2] : "obs";
                observandoTile = a.length > 5 ? new int[] {Integer.parseInt(a[3]), Integer.parseInt(a[4]), Integer.parseInt(a[5])} : null;
                tickConvidado = 0;
                return "true";
            }
            case "tile" -> {
                return tile(minecraft, a[1], Integer.parseInt(a[2]), Integer.parseInt(a[3]), Integer.parseInt(a[4]));
            }
            case "slot" -> {
                int slot = Integer.parseInt(a[1]);
                int button = a.length > 2 && a[2].equals("d") ? 1 : 0;
                // `slot N shift`: o shift-clique (`QUICK_MOVE`).
                var input = a.length > 2 && a[2].equals("shift") ? net.minecraft.world.inventory.ContainerInput.QUICK_MOVE : net.minecraft.world.inventory.ContainerInput.PICKUP;
                minecraft.gameMode.handleContainerInput(minecraft.player.containerMenu.containerId, slot, button, input, minecraft.player);
                return "true";
            }
            case "tipos" -> {
                StringBuilder out = new StringBuilder("[");
                boolean first = true;
                for (var holder : (Iterable<Holder.Reference<mekanism.api.upgrade.Upgrade>>) minecraft.level.registryAccess()
                      .lookupOrThrow(mekanism.api.MekanismRegistries.Keys.UPGRADES).listElements()::iterator) {
                    out.append(first ? "" : ",").append('"').append(holder.key().identifier()).append('"');
                    first = false;
                }
                return out.append("]").toString();
            }
            default -> {
                return "\"acao desconhecida " + a[0] + "\"";
            }
        }
    }

    private static void cursor(Minecraft minecraft, double guiX, double guiY) {
        try {
            double scale = minecraft.getWindow().getGuiScale();
            if (minecraft.gui.screen() instanceof mekanism.client.gui.IGuiWrapper wrapper) {
                guiX += wrapper.getLeftPos();
                guiY += wrapper.getTopPos();
            }
            var x = MouseHandler.class.getDeclaredField("xpos");
            var y = MouseHandler.class.getDeclaredField("ypos");
            x.setAccessible(true);
            y.setAccessible(true);
            x.setDouble(minecraft.mouseHandler, (guiX + 0.5) * scale);
            y.setDouble(minecraft.mouseHandler, (guiY + 0.5) * scale);
        } catch (ReflectiveOperationException e) {
            Mekanism.logger.error("dois: sem cursor", e);
        }
    }

    /// O tile do mundo do cliente (o que o jogo desenha e de onde o som lê): aceso, fator de volume e se está totalmente silenciado.
    private static String tile(Minecraft minecraft, String nome, int x, int y, int z) {
        var entity = minecraft.level.getBlockEntity(new BlockPos(x, y, z));
        String texto;
        if (entity instanceof TileEntityMekanism tile) {
            boolean mudo = false;
            try {
                Method method = TileEntityMekanism.class.getDeclaredMethod("isFullyMuffled");
                method.setAccessible(true);
                mudo = (boolean) method.invoke(tile);
            } catch (ReflectiveOperationException e) {
                Mekanism.logger.error("dois: sem isFullyMuffled", e);
            }
            texto = String.format(Locale.ROOT, "{\"nome\":\"%s\",\"gt\":%d,\"aceso\":%s,\"fator\":%.4f,\"mudo\":%s}", nome, minecraft.level.getGameTime(), tile.getActive(),
                  tile.getVolumeFactor(), mudo);
        } else {
            texto = String.format(Locale.ROOT, "{\"nome\":\"%s\",\"gt\":%d,\"tile\":null}", nome, minecraft.level.getGameTime());
        }
        log("tile " + texto);
        return texto;
    }

    private static String id(ItemStack stack) {
        return stack.isEmpty() ? "-" : BuiltInRegistries.ITEM.getKey(stack.getItem()) + "x" + stack.getCount();
    }

    /// O estado do cliente numa linha JSON: o tempo do mundo do cliente, a tela, os slots do menu com item, a experiência, e o tile da tela (energia, aceso, fator de volume).
    private static String ler(Minecraft minecraft, String nome) {
        StringBuilder out = new StringBuilder();
        out.append("{\"nome\":\"").append(nome).append("\",\"gt\":").append(minecraft.level.getGameTime());
        var screen = minecraft.gui.screen();
        out.append(",\"tela\":\"").append(screen == null ? "" : screen.getClass().getSimpleName()).append("\"");
        var menu = minecraft.player.containerMenu;
        out.append(",\"menu\":\"").append(menu.getClass().getSimpleName()).append("\",\"slots\":{");
        boolean first = true;
        for (int i = 0; i < menu.slots.size(); i++) {
            ItemStack stack = menu.slots.get(i).getItem();
            if (!stack.isEmpty()) {
                out.append(first ? "" : ",").append('"').append(i).append("\":\"").append(id(stack)).append('"');
                first = false;
            }
        }
        out.append("},\"carregado\":\"").append(id(menu.getCarried())).append("\"");
        out.append(",\"xp\":{\"total\":").append(minecraft.player.totalExperience).append(",\"nivel\":").append(minecraft.player.experienceLevel).append(",\"barra\":")
              .append(String.format(Locale.ROOT, "%.4f", minecraft.player.experienceProgress)).append("}");
        out.append(",\"cabeca\":\"").append(id(minecraft.player.getItemBySlot(net.minecraft.world.entity.EquipmentSlot.HEAD))).append("\"");
        if (screen instanceof mekanism.client.gui.GuiMekanismTile<?, ?> tela) {
            TileEntityMekanism tile = tela.getMenu().getTileEntity();
            out.append(",\"tile\":{\"aceso\":").append(tile.getActive()).append(",\"fator\":").append(String.format(Locale.ROOT, "%.4f", tile.getVolumeFactor()));
            if (tile.getEnergyContainer() instanceof BasicEnergyContainer container) {
                out.append(",\"energia\":").append(container.getAmountAsLong()).append(",\"capacidade\":").append(container.getCapacityAsLong());
            }
            out.append("}");
        }
        out.append("}");
        String texto = out.toString();
        log("ler " + texto);
        return texto;
    }

    // O anfitrião -----------------------------------------------------------------------------------------------------------------------------------------------

    private static void carrega() {
        roteiro = new ArrayList<>();
        try {
            for (String texto : Files.readAllLines(ANFITRIAO)) {
                String[] palavras = texto.trim().split("\\s+");
                if (palavras.length == 0 || palavras[0].isEmpty() || palavras[0].startsWith("#")) {
                    continue;
                }
                roteiro.add(palavras);
            }
        } catch (IOException e) {
            Mekanism.logger.error("dois: sem roteiro", e);
        }
    }

    private static void run(MinecraftServer server, String command) {
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command);
    }

    private static void anfitriao(Minecraft minecraft) {
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (server == null || ticks < WARMUP) {
            return;
        }
        // O vídeo com som: quem grava cria `som-video-mudo` se algum som sair do sink de teste, e o volume mestre volta a 0 na hora.
        if (Files.exists(SYNC.resolve("som-video-mudo")) && minecraft.options.getSoundSourceOptionInstance(net.minecraft.sounds.SoundSource.MASTER).get() > 0) {
            minecraft.options.getSoundSourceOptionInstance(net.minecraft.sounds.SoundSource.MASTER).set(0.0);
            log("volume mudo");
        }
        if (roteiro == null) {
            carrega();
            try {
                Files.createDirectories(SYNC);
                Files.deleteIfExists(COMANDOS);
                Files.deleteIfExists(RESPOSTAS);
                Files.writeString(COMANDOS, "");
            } catch (IOException e) {
                Mekanism.logger.error("dois: sync", e);
            }
        }
        if (espera > 0) {
            espera--;
            return;
        }
        if (esperandoResposta >= 0) {
            if (respondido(esperandoResposta)) {
                esperandoResposta = -1;
            } else {
                return;
            }
        }
        if (linha >= roteiro.size()) {
            minecraft.stop();
            return;
        }
        String[] a = roteiro.get(linha++);
        log("roteiro gt=" + server.overworld().getGameTime() + " " + String.join(" ", a));
        switch (a[0]) {
            case "lan" -> {
                // O servidor da LAN autentica os jogadores na Mojang, e o cliente de desenvolvimento não tem sessão: sem isto o convidado é recusado no login.
                server.setUsesAuthentication(false);
                boolean ok = server.isPublished() || server.publishServer(MinecraftServer.MultiplayerScope.LAN, true, Integer.parseInt(a[1]));
                log("lan " + a[1] + " " + ok);
            }
            case "espera_convidado" -> {
                if (server.getPlayerCount() < 2) {
                    linha--;
                    espera = 20;
                } else {
                    log("convidado entrou: " + server.getPlayerList().getPlayers().stream().map(p -> p.getName().getString()).toList());
                }
            }
            case "srv" -> {
                String comando = String.join(" ", java.util.Arrays.copyOfRange(a, 1, a.length));
                server.execute(() -> run(server, comando));
            }
            case "nbt" -> {
                // O tile do servidor inteiro (`saveWithFullMetadata`), numa linha do log: a configuração de lados, o dono e o modo de segurança.
                BlockPos at = new BlockPos(Integer.parseInt(a[2]), Integer.parseInt(a[3]), Integer.parseInt(a[4]));
                server.execute(() -> {
                    var entity = server.overworld().getBlockEntity(at);
                    log("nbt " + a[1] + " " + (entity == null ? "null" : entity.saveWithFullMetadata(server.overworld().registryAccess())));
                });
            }
            case "bloco" -> {
                // `bloco NOME X Y Z`: o estado do bloco no servidor (a direção de um bloco do Mekanism fica no estado, não no tile).
                BlockPos at = new BlockPos(Integer.parseInt(a[2]), Integer.parseInt(a[3]), Integer.parseInt(a[4]));
                server.execute(() -> log("bloco " + a[1] + " " + server.overworld().getBlockState(at)));
            }
            case "por" -> {
                // `por JOGADOR ITEM X`: o jogador põe o bloco do item em (X, -60, 3) clicando em cima do bloco de baixo (`ServerPlayerGameMode.useItemOn`), e fica o dono dele.
                int x = Integer.parseInt(a[3]);
                server.execute(() -> {
                    var player = server.getPlayerList().getPlayerByName(a[1]);
                    BlockPos pos = new BlockPos(x, Y, Z);
                    ItemStack stack = new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.parse(a[2])));
                    player.setItemInHand(InteractionHand.MAIN_HAND, stack);
                    BlockHitResult hit = new BlockHitResult(Vec3.atBottomCenterOf(pos), Direction.UP, pos.below(), false);
                    var result = player.gameMode.useItemOn(player, player.level(), stack, InteractionHand.MAIN_HAND, hit);
                    player.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
                    log("por " + a[1] + " " + a[2] + " " + pos + " -> " + result);
                });
            }
            case "freq" -> {
                // `freq NOME`: a tabela das frequências de segurança na memória do servidor (com o `dirty` dela) e o arquivo dela no disco.
                server.execute(() -> {
                    var lookup = mekanism.common.lib.frequency.FrequencyTypes.SECURITY.getLookup(null, mekanism.api.security.SecurityMode.PUBLIC);
                    var codec = mekanism.common.lib.frequency.FrequencyLookup.codec(mekanism.common.lib.frequency.FrequencyTypes.SECURITY,
                          mekanism.common.lib.frequency.FrequencyLookup.baseCodec(mekanism.common.lib.frequency.FrequencyTypes.SECURITY), mekanism.api.security.SecurityMode.PUBLIC);
                    var ops = net.minecraft.resources.RegistryOps.create(net.minecraft.nbt.NbtOps.INSTANCE, server.registryAccess());
                    log("freq " + a[1] + " memoria dirty=" + lookup.isDirty() + " " + codec.encodeStart(ops, lookup).getOrThrow());
                    Path raiz = server.getWorldPath(net.minecraft.world.level.storage.LevelResource.ROOT).resolve("data");
                    try (var arquivos = Files.walk(raiz)) {
                        List<Path> achados = arquivos.filter(caminho -> caminho.toString().contains("frequency") && caminho.toString().endsWith(".dat")).toList();
                        if (achados.isEmpty()) {
                            log("freq " + a[1] + " disco nenhum arquivo");
                        }
                        for (Path caminho : achados) {
                            log("freq " + a[1] + " disco " + raiz.relativize(caminho) + " " + net.minecraft.nbt.NbtIo.readCompressed(caminho, net.minecraft.nbt.NbtAccounter.unlimitedHeap()));
                        }
                    } catch (IOException e) {
                        log("freq " + a[1] + " disco erro " + e);
                    }
                });
            }
            case "congela" -> server.execute(() -> run(server, "tick freeze"));
            case "descongela" -> server.execute(() -> run(server, "tick unfreeze"));
            case "passos" -> {
                int n = Integer.parseInt(a[1]);
                server.execute(() -> run(server, "tick step " + n));
                espera = n + 10;
            }
            case "espera" -> espera = Integer.parseInt(a[1]);
            case "caixa" -> {
                int x = Integer.parseInt(a[1]);
                server.execute(() -> box(server, x));
            }
            case "montar" -> {
                int x = Integer.parseInt(a[2]);
                server.execute(() -> build(server, a[1], x));
            }
            case "estado" -> {
                int x = Integer.parseInt(a[1]);
                Map<String, String> args = new HashMap<>();
                for (int i = 2; i < a.length; i++) {
                    String[] par = a[i].split("=", 2);
                    args.put(par[0], par.length > 1 ? par[1] : "");
                }
                server.execute(() -> state(server, x, args));
            }
            case "silencio" -> {
                int x = Integer.parseInt(a[1]);
                int count = Integer.parseInt(a[2]);
                server.execute(() -> setMuffling(server, x, count));
            }
            case "liga" -> {
                int x = Integer.parseInt(a[1]);
                server.execute(() -> power(server, x, true));
            }
            case "desliga" -> {
                int x = Integer.parseInt(a[1]);
                server.execute(() -> power(server, x, false));
            }
            case "anf" -> {
                String resposta = acao(minecraft, java.util.Arrays.copyOfRange(a, 1, a.length));
                log("anf " + a[1] + " -> " + resposta);
            }
            case "conv" -> {
                int id = proximoId++;
                try {
                    Files.writeString(COMANDOS, id + " " + String.join(" ", java.util.Arrays.copyOfRange(a, 1, a.length)) + "\n", StandardOpenOption.APPEND);
                } catch (IOException e) {
                    Mekanism.logger.error("dois: comando", e);
                }
                esperandoResposta = id;
            }
            case "fim" -> {
                log("fim");
                minecraft.stop();
            }
            case "volume_se_ok" -> {
                // `volume_se_ok CATEGORIA...`: o mestre e as categorias pedidas em 1, só se quem grava já conferiu que o som do jogo está no sink de teste (`som-video-ok`).
                boolean ok = Files.exists(SYNC.resolve("som-video-ok")) && !Files.exists(SYNC.resolve("som-video-mudo"));
                if (ok) {
                    for (int i = 1; i < a.length; i++) {
                        minecraft.options.getSoundSourceOptionInstance(net.minecraft.sounds.SoundSource.valueOf(a[i].toUpperCase(java.util.Locale.ROOT))).set(1.0);
                    }
                    minecraft.options.getSoundSourceOptionInstance(net.minecraft.sounds.SoundSource.MASTER).set(1.0);
                }
                log("volume " + ok);
            }
            case "mudo" -> {
                minecraft.options.getSoundSourceOptionInstance(net.minecraft.sounds.SoundSource.MASTER).set(0.0);
                log("mudo");
            }
            case "itens" -> {
                // `itens NOME X Y Z`: os itens soltos num raio de 8 de (X, Y, Z) no servidor, com a posição relativa a esse bloco.
                BlockPos at = new BlockPos(Integer.parseInt(a[2]), Integer.parseInt(a[3]), Integer.parseInt(a[4]));
                server.execute(() -> {
                    StringBuilder out = new StringBuilder();
                    for (var entity : server.overworld().getEntitiesOfClass(net.minecraft.world.entity.item.ItemEntity.class, new net.minecraft.world.phys.AABB(at).inflate(8))) {
                        out.append(BuiltInRegistries.ITEM.getKey(entity.getItem().getItem())).append(' ').append(entity.getItem().getCount()).append(" @ ")
                              .append(entity.getX() - at.getX()).append(' ').append(entity.getY() - at.getY()).append(' ').append(entity.getZ() - at.getZ()).append("; ");
                    }
                    log("itens " + a[1] + " " + out);
                });
            }
            default -> log("acao desconhecida " + a[0]);
        }
    }

    private static boolean respondido(int id) {
        try {
            if (!Files.exists(RESPOSTAS)) {
                return false;
            }
            for (String texto : Files.readAllLines(RESPOSTAS)) {
                if (texto.startsWith("{\"id\":" + id + ",")) {
                    log("resposta " + texto);
                    return true;
                }
            }
        } catch (IOException e) {
            Mekanism.logger.error("dois: respostas", e);
        }
        return false;
    }

    // A montagem (a mesma da CenaTelas) ----------------------------------------------------------------------------------------------------------------------------

    private static void box(MinecraftServer server, int x) {
        run(server, "fill " + (x - 6) + " -61 -6 " + (x + 6) + " -50 10 minecraft:air");
        run(server, "fill " + (x - 6) + " -61 -6 " + (x + 6) + " -50 10 minecraft:black_concrete hollow");
        run(server, "fillbiome " + (x - 6) + " -61 -6 " + (x + 6) + " -50 10 minecraft:plains");
    }

    private static BlockState block(String machine) {
        return switch (machine) {
            case "chamber" -> MekanismBlocks.ENRICHMENT_CHAMBER.defaultState();
            case "crusher" -> MekanismBlocks.CRUSHER.defaultState();
            case "smelter" -> MekanismBlocks.ENERGIZED_SMELTER.defaultState();
            case "infuser" -> MekanismBlocks.METALLURGIC_INFUSER.defaultState();
            case "generator" -> GeneratorsBlocks.HEAT_GENERATOR.defaultState();
            default -> throw new IllegalArgumentException(machine);
        };
    }

    private static TileEntityMekanism tile(MinecraftServer server, int x) {
        return (TileEntityMekanism) server.overworld().getBlockEntity(new BlockPos(x, Y, Z));
    }

    private static void build(MinecraftServer server, String machine, int x) {
        ServerLevel level = server.overworld();
        level.setBlockAndUpdate(new BlockPos(x, Y, Z), Blocks.AIR.defaultBlockState());
        level.setBlockAndUpdate(new BlockPos(x, Y, Z), block(machine));
        TileEntityMekanism tile = tile(server, x);
        tile.applyComponents(net.minecraft.core.component.DataComponentMap.EMPTY, net.minecraft.core.component.DataComponentPatch.EMPTY);
    }

    private static void state(MinecraftServer server, int x, Map<String, String> args) {
        ServerLevel level = server.overworld();
        TileEntityMekanism tile = tile(server, x);
        boolean infuser = tile instanceof TileEntityMetallurgicInfuser;
        if (args.containsKey("entrada")) {
            setItem(tile, infuser ? 1 : 0, args.get("entrada"));
        }
        if (args.containsKey("saida")) {
            setItem(tile, infuser ? 2 : 1, args.get("saida"));
        }
        if (args.containsKey("quimico") && tile instanceof TileEntityMetallurgicInfuser inf) {
            String[] parts = args.get("quimico").split(":");
            var key = switch (parts[0]) {
                case "bio" -> ChemicalIds.BIO;
                case "carbon" -> ChemicalIds.CARBON;
                default -> throw new IllegalArgumentException(parts[0]);
            };
            Holder<Chemical> chemical = level.registryAccess().getOrThrow(key);
            inf.infusionTank.setContents(ChemicalResource.of(chemical), Long.parseLong(parts[1]), null);
        }
        if (args.containsKey("lava") && tile instanceof TileEntityHeatGenerator generator) {
            generator.lavaTank.setContents(FluidResource.of(net.minecraft.world.level.material.Fluids.LAVA), Long.parseLong(args.get("lava")), null);
        }
        var component = tile.getComponent();
        if (component != null && args.containsKey("upgrades")) {
            for (String spec : args.get("upgrades").split(",")) {
                String[] parts = spec.split(":");
                var holder = level.registryAccess().getOrThrow(
                      net.minecraft.resources.ResourceKey.create(mekanism.api.MekanismRegistries.Keys.UPGRADES, Mekanism.rl(parts[0])));
                component.addUpgrades(level.registryAccess(), holder, Integer.parseInt(parts[1]));
            }
        }
        if (args.containsKey("energia")) {
            BasicEnergyContainer container = (BasicEnergyContainer) tile.getEnergyContainer();
            String value = args.get("energia");
            container.setEnergy(value.equals("cap") ? container.getCapacityAsLong() : Long.parseLong(value), null);
        }
    }

    private static void setMuffling(MinecraftServer server, int x, int count) {
        TileEntityMekanism tile = tile(server, x);
        Holder<mekanism.api.upgrade.Upgrade> upgrade = server.overworld().registryAccess().getOrThrow(
              net.minecraft.resources.ResourceKey.create(mekanism.api.MekanismRegistries.Keys.UPGRADES, Mekanism.rl("muffling")));
        int have = tile.getComponent().getUpgrades(upgrade);
        if (count > have) {
            tile.getComponent().addUpgrades(server.overworld().registryAccess(), upgrade, count - have);
        } else if (count < have) {
            tile.getComponent().removeUpgrade(server.overworld().registryAccess(), upgrade, count == 0);
        }
    }

    private static void power(MinecraftServer server, int x, boolean on) {
        TileEntityMekanism tile = tile(server, x);
        BasicEnergyContainer container = (BasicEnergyContainer) tile.getEnergyContainer();
        if (!on) {
            container.setEnergy(0, null);
            return;
        }
        var coal = BuiltInRegistries.ITEM.getValue(Identifier.parse("minecraft:coal"));
        tile.getInventorySlots().get(0).setContents(ItemResource.of(coal), 64, null);
        container.setEnergy(container.getCapacityAsLong(), null);
    }

    private static void setItem(TileEntityMekanism tile, int slot, String spec) {
        String[] parts = spec.split(":");
        String id = parts.length == 3 ? parts[0] + ":" + parts[1] : "minecraft:" + parts[0];
        int count = Integer.parseInt(parts[parts.length - 1]);
        var item = BuiltInRegistries.ITEM.getValue(Identifier.parse(id));
        tile.getInventorySlots().get(slot).setContents(ItemResource.of(item), count, null);
    }
}
