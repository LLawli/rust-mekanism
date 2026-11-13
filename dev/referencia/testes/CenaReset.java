package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.TimeUnit;
import java.util.function.BooleanSupplier;
import mekanism.api.MekanismAPI;
import mekanism.common.Mekanism;
import mekanism.common.content.network.EnergyNetwork;
import mekanism.common.content.network.transmitter.ThermodynamicConductor;
import mekanism.common.content.network.transmitter.Transmitter;
import mekanism.common.content.network.transmitter.UniversalCable;
import mekanism.common.lib.transmitter.DynamicBufferedResourceNetwork;
import mekanism.common.lib.transmitter.DynamicNetwork;
import mekanism.common.lib.transmitter.TransmitterNetworkRegistry;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tile.transmitter.TileEntityTransmitter;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.chat.Component;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.Difficulty;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.levelgen.WorldOptions;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.event.level.LevelEvent;
import net.neoforged.neoforge.event.server.ServerStartedEvent;
import net.neoforged.neoforge.event.server.ServerStoppedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.event.server.ServerStoppingEvent;

/// O mundo que sai e volta no cliente oficial (frente `reset-mundo`, decisão 183 (c)): o que o original faz com o estado estático das redes (o `TransmitterNetworkRegistry`, o
/// `TransporterManager`, o `PathfinderCache`, o `clientNetworks`, o `MekanismAPI.debug`) ao sair do mundo para o menu, ao abrir o mesmo mundo e um segundo mundo, no mesmo processo, e ao
/// trocar de dimensão. Só age com o arquivo `cena-reset` ao lado (o conteúdo é ignorado). O cliente nasce no título (o `GameTestWorld` sai do caminho, o `oficial_reset.sh` cuida disso)
/// e a cena cria `ResetA` e `ResetB` (superplano, semente fixa, criativo, pacífico). Cada `snap` escreve uma linha de `reset-mundo.jsonl`: o registro do servidor (as redes na ordem do
/// conjunto, com o conteúdo, `networksToChange`, `invalidTransmitters`, `dimensionStore`), o `clientNetworks`, as pilhas em trânsito (`TransporterManager`) e o `PathfinderCache`, mais o
/// estado dos blocos da cena (a parte do cabo, o calor, o NBT). Os eventos do servidor (`ServerStoppingEvent`, `ChunkEvent.Unload`, `LevelEvent.Unload`, `ServerStoppedEvent` antes e
/// depois do `Mekanism.serverStopped`) também escrevem uma linha, com o estado do registro na hora.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaReset {

    private static final Path ATIVA = Path.of("cena-reset");
    private static final Path SAIDA = Path.of("reset-mundo.jsonl");
    private static final String A = "ResetA";
    private static final String B = "ResetB";
    private static final int Y = -61;

    private static final List<BooleanSupplier> passos = new ArrayList<>();
    private static boolean montado;
    private static boolean done;
    private static int indice;
    private static int n;
    private static int entrou;
    private static String rotulo = "";

    private static void escreve(String linha) {
        try {
            Files.writeString(SAIDA, linha + "\n", StandardOpenOption.CREATE, StandardOpenOption.APPEND);
        } catch (IOException e) {
            Mekanism.logger.error("cena-reset: escrita", e);
        }
    }

    private static String q(String text) {
        return "\"" + text.replace("\\", "\\\\").replace("\"", "\\\"").replace("\n", "\\n") + "\"";
    }

    private static Object campo(Class<?> type, String name, Object target) throws ReflectiveOperationException {
        Field field = type.getDeclaredField(name);
        field.setAccessible(true);
        return field.get(target);
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // O estado
    // ---------------------------------------------------------------------------------------------------------------------

    private static String rede(DynamicNetwork<?, ?, ?> network) throws ReflectiveOperationException {
        StringBuilder sb = new StringBuilder("{\"k\":").append(q(network.getClass().getSimpleName()))
              .append(",\"tx\":").append(network.transmittersSize()).append(",\"acc\":").append(network.getAcceptorCount());
        if (network instanceof EnergyNetwork energy) {
            sb.append(",\"energia\":").append(energy.energyContainer.getAmountAsLong()).append(",\"buffer\":").append(energy.getBuffer());
        } else if (network instanceof DynamicBufferedResourceNetwork<?, ?, ?, ?> resource) {
            sb.append(",\"buffer\":").append(q(String.valueOf(resource.getBuffer())));
        } else if (network.getClass().getSimpleName().equals("HeatNetwork")) {
            sb.append(",\"temperatura\":").append(campo(network.getClass(), "meanTemp", network));
        }
        return sb.append('}').toString();
    }

    private static String redes(Iterable<DynamicNetwork<?, ?, ?>> networks) throws ReflectiveOperationException {
        StringBuilder sb = new StringBuilder("[");
        boolean first = true;
        for (DynamicNetwork<?, ?, ?> network : networks) {
            sb.append(first ? "" : ",").append(rede(network));
            first = false;
        }
        return sb.append(']').toString();
    }

    @SuppressWarnings("unchecked")
    private static String registro() {
        try {
            Class<?> registry = TransmitterNetworkRegistry.class;
            Set<DynamicNetwork<?, ?, ?>> networks = (Set<DynamicNetwork<?, ?, ?>>) campo(registry, "networks", null);
            Set<?> change = (Set<?>) campo(registry, "networksToChange", null);
            Set<?> invalid = (Set<?>) campo(registry, "invalidTransmitters", null);
            Map<?, ?> store = (Map<?, ?>) campo(registry, "dimensionStore", null);
            StringBuilder dims = new StringBuilder("{");
            boolean first = true;
            for (Map.Entry<?, ?> entry : store.entrySet()) {
                Object data = entry.getValue();
                Class<?> type = data.getClass();
                Method isEmpty = campo(type, "transmitters", data).getClass().getMethod("isEmpty");
                dims.append(first ? "" : ",").append(q(String.valueOf(((net.minecraft.resources.ResourceKey<?>) entry.getKey()).identifier()))).append(":{\"transmissores_vazio\":")
                      .append(isEmpty.invoke(campo(type, "transmitters", data))).append(",\"orfaos\":").append(((Map<?, ?>) campo(type, "newOrphanTransmitters", data)).size())
                      .append(",\"chunks_mudados\":").append(((Map<?, ?>) campo(type, "changedTicketChunks", data)).size()).append('}');
                first = false;
            }
            dims.append('}');
            return "{\"n\":" + networks.size() + ",\"lista\":" + redes(networks) + ",\"networksToChange\":" + change.size() + ",\"invalid\":" + invalid.size() + ",\"dimensionStore\":" + dims + "}";
        } catch (ReflectiveOperationException | RuntimeException e) {
            return "{\"erro\":" + q(String.valueOf(e)) + "}";
        }
    }

    @SuppressWarnings("unchecked")
    private static String cliente() {
        try {
            Map<?, DynamicNetwork<?, ?, ?>> client = (Map<?, DynamicNetwork<?, ?, ?>>) campo(TransmitterNetworkRegistry.class, "clientNetworks", null);
            return "{\"n\":" + client.size() + ",\"lista\":" + redes(client.values()) + "}";
        } catch (ReflectiveOperationException | RuntimeException e) {
            return "{\"erro\":" + q(String.valueOf(e)) + "}";
        }
    }

    private static String transito() {
        try {
            Map<?, ?> flowing = (Map<?, ?>) campo(mekanism.common.content.transporter.TransporterManager.class, "flowingStacks", null);
            int stacks = 0;
            for (Object journal : flowing.values()) {
                stacks += ((Set<?>) campo(journal.getClass(), "stacks", journal)).size();
            }
            Map<?, ?> cache = (Map<?, ?>) campo(mekanism.common.content.transporter.PathfinderCache.class, "cachedPaths", null);
            int paths = 0;
            for (Object value : cache.values()) {
                paths += ((Map<?, ?>) value).size();
            }
            return "{\"destinos\":" + flowing.size() + ",\"pilhas\":" + stacks + ",\"redes_no_cache\":" + cache.size() + ",\"caminhos\":" + paths + "}";
        } catch (ReflectiveOperationException | RuntimeException e) {
            return "{\"erro\":" + q(String.valueOf(e)) + "}";
        }
    }

    /// Os blocos da cena no nível, se o chunk está carregado: a parte de cada cabo, o calor de cada condutor, as pilhas de cada transportador e o NBT de uma peça por linha.
    private static String blocos(ServerLevel level, int baseZ, int from, int to, String tipo) {
        StringBuilder sb = new StringBuilder("{\"tipo\":" + q(tipo) + ",\"pecas\":[");
        for (int x = from; x <= to; x++) {
            BlockPos pos = new BlockPos(x, Y, baseZ);
            sb.append(x == from ? "" : ",");
            if (!level.hasChunkAt(pos)) {
                sb.append("\"sem_chunk\"");
                continue;
            }
            BlockEntity be = level.getBlockEntity(pos);
            if (!(be instanceof TileEntityTransmitter tile)) {
                sb.append("null");
                continue;
            }
            Transmitter<?, ?, ?> transmitter = tile.getTransmitter();
            sb.append("{\"x\":").append(x).append(",\"carregado\":").append(tile.isLoaded()).append(",\"rede\":").append(transmitter.hasTransmitterNetwork());
            try {
                if (transmitter instanceof UniversalCable cable) {
                    sb.append(",\"parte\":").append(cable.getShare());
                } else if (transmitter instanceof ThermodynamicConductor conductor) {
                    sb.append(",\"calor\":").append(conductor.buffer.getHeat());
                } else if (transmitter instanceof mekanism.common.content.network.transmitter.LogisticalTransporterBase transporter) {
                    sb.append(",\"transito\":").append(((Map<?, ?>) campo(mekanism.common.content.network.transmitter.LogisticalTransporterBase.class, "transit", transporter)).size());
                }
            } catch (ReflectiveOperationException | RuntimeException e) {
                sb.append(",\"erro\":").append(q(String.valueOf(e)));
            }
            if (x == from) {
                sb.append(",\"nbt\":").append(q(tile.saveWithFullMetadata(level.registryAccess()).toString()));
            }
            sb.append('}');
        }
        return sb.append("]}").toString();
    }

    private static String servidor(MinecraftServer server) {
        StringBuilder sb = new StringBuilder("{\"registro\":").append(registro()).append(",\"transito\":").append(transito());
        if (server != null) {
            ServerLevel level = server.overworld();
            sb.append(",\"blocos\":[").append(blocos(level, 0, 0, 4, "cabo")).append(',').append(blocos(level, 2, 0, 4, "cano")).append(',').append(blocos(level, 4, 0, 4, "tubo"))
                  .append(',').append(blocos(level, 6, 0, 4, "condutor")).append(',').append(blocos(level, 8, 1, 8, "transportador")).append(']');
        }
        return sb.append('}').toString();
    }

    private static void snap(Minecraft minecraft, String label) {
        MinecraftServer server = minecraft.getSingleplayerServer();
        String servidor;
        if (server != null && server.isRunning()) {
            try {
                servidor = server.submit(() -> servidor(server)).get(20, TimeUnit.SECONDS);
            } catch (Exception e) {
                servidor = "{\"erro\":" + q(String.valueOf(e)) + "}";
            }
        } else {
            servidor = servidor(null);
        }
        long gt = minecraft.level == null ? -1 : minecraft.level.getGameTime();
        escreve("{\"e\":\"snap\",\"rotulo\":" + q(label) + ",\"gt\":" + gt + ",\"depuracao\":" + MekanismAPI.debug + ",\"servidor_ativo\":" + (server != null) + ",\"servidor\":" + servidor
              + ",\"cliente\":" + cliente() + "}");
        Mekanism.logger.info("CENA-RESET {}", label);
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // Os eventos do servidor
    // ---------------------------------------------------------------------------------------------------------------------

    private static int descargas;

    /// Os primeiros ticks do servidor depois de abrir o mundo: quantas redes o registro tem no fim de cada um (depois do `onTick` do Mekanism, que corre na prioridade normal).
    private static int primeiros = -1;

    @SubscribeEvent
    public static void onStarted(ServerStartedEvent event) {
        if (Files.exists(ATIVA)) {
            fechando = false;
            primeiros = 0;
            escreve("{\"e\":\"started\",\"registro\":" + registro() + "}");
        }
    }

    /// O que o servidor tem nos primeiros ticks (decisão 195): quantos jogadores, os chunks carregados de cada nível, e se o chunk (0, 0) do Overworld anda (`isPositionTicking`, `isPositionEntityTicking`).
    private static String primeiroEstado(MinecraftServer server) {
        StringBuilder carregados = new StringBuilder("{");
        boolean first = true;
        for (ServerLevel level : server.getAllLevels()) {
            carregados.append(first ? "" : ",").append(q(String.valueOf(level.dimension().identifier()))).append(':').append(level.getChunkSource().getLoadedChunksCount());
            first = false;
        }
        carregados.append('}');
        ServerLevel overworld = server.overworld();
        return "\"ms\":" + System.currentTimeMillis() + ",\"jogadores\":" + server.getPlayerCount() + ",\"carregados\":" + carregados + ",\"anda00\":" + overworld.getChunkSource().isPositionTicking(0L)
            + ",\"entidades00\":" + overworld.isPositionEntityTicking(BlockPos.ZERO);
    }

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public static void onServerTickPre(ServerTickEvent.Pre event) {
        if (primeiros >= 0 && primeiros < 14 && Files.exists(ATIVA)) {
            escreve("{\"e\":\"tick_pre\",\"k\":" + (primeiros + 1) + ",\"gt\":" + event.getServer().overworld().getGameTime() + "," + primeiroEstado(event.getServer()) + "}");
        }
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    public static void onServerTick(ServerTickEvent.Post event) {
        if (primeiros >= 0 && primeiros < 14 && Files.exists(ATIVA)) {
            primeiros++;
            escreve("{\"e\":\"tick\",\"k\":" + primeiros + ",\"gt\":" + event.getServer().overworld().getGameTime() + "," + primeiroEstado(event.getServer()) + ",\"registro\":" + registro() + "}");
        }
        if (obs > 0 && Files.exists(ATIVA)) {
            obs--;
            obsK++;
            MinecraftServer server = event.getServer();
            StringBuilder carregados = new StringBuilder("{");
            boolean first = true;
            for (ServerLevel level : server.getAllLevels()) {
                carregados.append(first ? "" : ",").append(q(String.valueOf(level.dimension().identifier()))).append(':').append(level.getChunkSource().getLoadedChunksCount());
                first = false;
            }
            carregados.append('}');
            StringBuilder desc = new StringBuilder("{");
            first = true;
            for (Map.Entry<String, Integer> entry : descPorDimensao.entrySet()) {
                desc.append(first ? "" : ",").append(q(entry.getKey())).append(':').append(entry.getValue());
                first = false;
            }
            desc.append('}');
            escreve("{\"e\":\"obs\",\"tag\":" + q(obsTag) + ",\"k\":" + obsK + ",\"gt\":" + server.overworld().getGameTime() + ",\"ms\":" + System.currentTimeMillis()
                  + ",\"descarregados\":" + desc + ",\"carregados\":" + carregados + ",\"perto\":[" + obsPerto + "],\"redes\":" + tamanho("networks")
                  + ",\"invalid\":" + tamanho("invalidTransmitters") + ",\"networksToChange\":" + tamanho("networksToChange") + ",\"transito\":" + transito() + "}");
            descPorDimensao.clear();
            obsPerto.setLength(0);
        }
    }

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public static void onStopping(ServerStoppingEvent event) {
        if (!Files.exists(ATIVA)) {
            return;
        }
        descargas = 0;
        escreve("{\"e\":\"stopping\",\"rotulo\":" + q(rotulo) + ",\"registro\":" + registro() + ",\"transito\":" + transito() + "}");
    }

    /// A janela de observação tick a tick (descarte da dimensão que o jogador deixa): quantos ticks faltam, o rótulo, e o que descarregou desde a última linha.
    private static int obs;
    private static int obsK;
    private static String obsTag = "";
    private static final Map<String, Integer> descPorDimensao = new LinkedHashMap<>();
    private static final StringBuilder obsPerto = new StringBuilder();
    /// Verdadeiro do `sai` até o próximo mundo abrir: cada descarga de chunk perto da origem escreve uma linha (a ordem e o `invalid` na hora).
    private static boolean fechando;

    private static int tamanho(String campoNome) {
        try {
            return ((Set<?>) campo(TransmitterNetworkRegistry.class, campoNome, null)).size();
        } catch (ReflectiveOperationException | RuntimeException e) {
            return -1;
        }
    }

    @SubscribeEvent
    public static void onChunkUnload(ChunkEvent.Unload event) {
        if (Files.exists(ATIVA) && !event.getLevel().isClientSide()) {
            descargas++;
            String dim = String.valueOf(((Level) event.getLevel()).dimension().identifier());
            descPorDimensao.merge(dim, 1, Integer::sum);
            int x = event.getChunk().getPos().x();
            int z = event.getChunk().getPos().z();
            if (Math.abs(x) <= 1 && Math.abs(z) <= 1) {
                if (obs > 0) {
                    obsPerto.append(obsPerto.length() == 0 ? "" : ",").append("{\"dim\":").append(q(dim)).append(",\"x\":").append(x).append(",\"z\":").append(z)
                          .append(",\"invalid\":").append(tamanho("invalidTransmitters")).append('}');
                }
                if (fechando) {
                    escreve("{\"e\":\"unload_perto\",\"rotulo\":" + q(rotulo) + ",\"dim\":" + q(dim) + ",\"x\":" + x + ",\"z\":" + z + ",\"n\":" + descargas
                          + ",\"invalid\":" + tamanho("invalidTransmitters") + ",\"networksToChange\":" + tamanho("networksToChange") + "}");
                }
            }
        }
    }

    private static void marca(String nome, MinecraftServer server) {
        escreve("{\"e\":\"marca\",\"nome\":" + q(nome) + ",\"gt\":" + server.overworld().getGameTime() + ",\"ms\":" + System.currentTimeMillis() + "}");
    }

    @SubscribeEvent
    public static void onLevelUnload(LevelEvent.Unload event) {
        if (Files.exists(ATIVA) && !event.getLevel().isClientSide()) {
            escreve("{\"e\":\"level_unload\",\"rotulo\":" + q(rotulo) + ",\"nivel\":" + q(String.valueOf(((Level) event.getLevel()).dimension().identifier())) + ",\"chunks_descarregados\":" + descargas
                  + ",\"registro\":" + registro() + "}");
        }
    }

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public static void onStoppedAntes(ServerStoppedEvent event) {
        if (Files.exists(ATIVA)) {
            escreve("{\"e\":\"stopped_antes\",\"rotulo\":" + q(rotulo) + ",\"chunks_descarregados\":" + descargas + ",\"registro\":" + registro() + ",\"transito\":" + transito() + "}");
        }
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    public static void onStoppedDepois(ServerStoppedEvent event) {
        if (Files.exists(ATIVA)) {
            escreve("{\"e\":\"stopped_depois\",\"rotulo\":" + q(rotulo) + ",\"registro\":" + registro() + ",\"transito\":" + transito() + "}");
        }
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // Os passos
    // ---------------------------------------------------------------------------------------------------------------------

    private static boolean noMundo(Minecraft minecraft) {
        return minecraft.level != null && minecraft.player != null && minecraft.getSingleplayerServer() != null;
    }

    private static void run(MinecraftServer server, String command) {
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command);
    }

    private static BooleanSupplier espera(int ticks) {
        return () -> ++n >= ticks;
    }

    private static BooleanSupplier faz(Runnable action) {
        return () -> {
            action.run();
            return true;
        };
    }

    /// Cria o mundo (superplano, semente fixa) a partir do título e espera o jogador entrar.
    private static BooleanSupplier cria(String nome, long semente) {
        return () -> {
            Minecraft minecraft = Minecraft.getInstance();
            if (noMundo(minecraft)) {
                return true;
            }
            if (minecraft.level == null && minecraft.getSingleplayerServer() == null && ++n == 30) {
                escreve("{\"e\":\"titulo\",\"tela\":" + q(String.valueOf(minecraft.gui.screen())) + "}");
            }
            if (minecraft.level == null && minecraft.getSingleplayerServer() == null && n == 60) {
                escreve("{\"e\":\"criando\",\"nome\":" + q(nome) + "}");
                minecraft.createWorldOpenFlows().createFreshLevel(nome,
                      new LevelSettings(nome, GameType.CREATIVE, new LevelSettings.DifficultySettings(Difficulty.PEACEFUL, false, false), true, WorldDataConfiguration.DEFAULT),
                      new WorldOptions(semente, false, false),
                      registries -> registries.lookupOrThrow(Registries.WORLD_PRESET).getOrThrow(WorldPresets.FLAT).value().createWorldDimensions(),
                      new TitleScreen());
            }
            return false;
        };
    }

    private static BooleanSupplier abre(String nome) {
        return () -> {
            Minecraft minecraft = Minecraft.getInstance();
            if (noMundo(minecraft)) {
                return true;
            }
            if (minecraft.level == null && minecraft.getSingleplayerServer() == null && ++n == 20) {
                escreve("{\"e\":\"abrindo\",\"nome\":" + q(nome) + "}");
                minecraft.createWorldOpenFlows().openWorld(nome, () -> minecraft.setScreenAndShow(new TitleScreen()));
            }
            return false;
        };
    }

    /// Conta os ticks desde que o jogador entrou: o passo seguinte só roda depois de `ticks` deles.
    private static BooleanSupplier depoisDeEntrar(int ticks) {
        return () -> {
            Minecraft minecraft = Minecraft.getInstance();
            if (!noMundo(minecraft)) {
                return false;
            }
            return ++entrou >= ticks;
        };
    }

    private static BooleanSupplier zera() {
        return () -> {
            entrou = 0;
            return true;
        };
    }

    private static BooleanSupplier sai(String nome) {
        return () -> {
            Minecraft minecraft = Minecraft.getInstance();
            if (minecraft.level != null) {
                rotulo = nome;
                fechando = true;
                escreve("{\"e\":\"saindo\",\"rotulo\":" + q(nome) + "}");
                // O que o `PauseScreen` faz no botão de sair do mundo local.
                minecraft.level.disconnect(Component.translatable("menu.disconnect"));
                minecraft.disconnectWithSavingScreen();
                minecraft.setScreenAndShow(new TitleScreen());
            }
            return true;
        };
    }

    private static BooleanSupplier noMenu() {
        return () -> Minecraft.getInstance().level == null && Minecraft.getInstance().getSingleplayerServer() == null;
    }

    private static BooleanSupplier snapPasso(String label) {
        return faz(() -> snap(Minecraft.getInstance(), label));
    }

    private static void servidorFaz(Consumer1 action) {
        Minecraft minecraft = Minecraft.getInstance();
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (server == null) {
            return;
        }
        try {
            server.submit(() -> {
                try {
                    action.accept(server);
                } catch (Exception e) {
                    throw new IllegalStateException(e);
                }
                return "ok";
            }).get(20, TimeUnit.SECONDS);
        } catch (Exception e) {
            escreve("{\"e\":\"erro\",\"onde\":\"servidorFaz\",\"erro\":" + q(String.valueOf(e)) + "}");
        }
    }

    private interface Consumer1 {
        void accept(MinecraftServer server) throws Exception;
    }

    private static void linha(MinecraftServer server, String block, int z, int from, int to) {
        for (int x = from; x <= to; x++) {
            run(server, String.format(Locale.ROOT, "setblock %d %d %d %s", x, Y, z, block));
        }
    }

    private static void config(MinecraftServer server) {
        run(server, "difficulty peaceful");
        run(server, "gamerule advance_time false");
        run(server, "gamerule advance_weather false");
        run(server, "gamerule send_command_feedback false");
        run(server, "time set 6000");
        run(server, "weather clear");
        run(server, "gamemode creative @a");
        run(server, "tp @a 2.5 -60 -6");
    }

    private static void constroiA(MinecraftServer server) {
        linha(server, "mekanism:basic_universal_cable", 0, 0, 4);
        linha(server, "mekanism:basic_mechanical_pipe", 2, 0, 4);
        linha(server, "mekanism:basic_pressurized_tube", 4, 0, 4);
        linha(server, "mekanism:basic_thermodynamic_conductor", 6, 0, 4);
        linha(server, "mekanism:basic_logistical_transporter", 8, 1, 8);
        run(server, String.format(Locale.ROOT, "setblock 9 %d 8 minecraft:chest", Y));
        // O calor: um Heat Generator em cima do último condutor (o calor dele sai pelo condutor, como na cena do port).
        run(server, String.format(Locale.ROOT, "setblock 4 %d 6 mekanismgenerators:heat_generator", Y + 1));
        // Os itens: um baú sobre um funil sobre o primeiro transportador (o funil passa um item a cada 8 ticks).
        run(server, String.format(Locale.ROOT, "setblock 1 %d 8 minecraft:hopper[facing=down]", Y + 1));
        run(server, String.format(Locale.ROOT, "setblock 1 %d 8 minecraft:chest", Y + 2));
    }

    private static void constroiB(MinecraftServer server) {
        linha(server, "mekanism:basic_universal_cable", 0, 0, 2);
    }

    @SuppressWarnings("unchecked")
    private static void enche(MinecraftServer server) throws Exception {
        Set<DynamicNetwork<?, ?, ?>> networks = (Set<DynamicNetwork<?, ?, ?>>) campo(TransmitterNetworkRegistry.class, "networks", null);
        for (DynamicNetwork<?, ?, ?> network : new ArrayList<>(networks)) {
            if (network instanceof EnergyNetwork energy) {
                energy.energyContainer.setEnergy(12_345, null);
            }
        }
    }

    // O preenchimento dos tanques e do calor é feito pelos passos abaixo, por reflexão, porque o tipo do recurso muda por rede.
    private static void enchePorReflexao(MinecraftServer server, String familia, String recurso, long quantidade) throws Exception {
        @SuppressWarnings("unchecked")
        Set<DynamicNetwork<?, ?, ?>> networks = (Set<DynamicNetwork<?, ?, ?>>) campo(TransmitterNetworkRegistry.class, "networks", null);
        for (DynamicNetwork<?, ?, ?> network : new ArrayList<>(networks)) {
            if (!network.getClass().getSimpleName().equals(familia)) {
                continue;
            }
            Object container = campo(DynamicBufferedResourceNetwork.class, "container", network);
            Object resource;
            if (familia.equals("FluidNetwork")) {
                resource = net.neoforged.neoforge.transfer.fluid.FluidResource.of(net.minecraft.world.level.material.Fluids.WATER);
            } else {
                var key = net.minecraft.resources.ResourceKey.create(mekanism.api.MekanismRegistries.Keys.CHEMICAL, net.minecraft.resources.Identifier.parse(recurso));
                resource = mekanism.api.chemical.ChemicalResource.of(server.registryAccess().getOrThrow(key));
            }
            for (Method method : container.getClass().getMethods()) {
                if (method.getName().equals("setContents") && method.getParameterCount() == 3 && method.getParameterTypes()[1] == long.class) {
                    method.invoke(container, resource, quantidade, null);
                    break;
                }
            }
        }
    }

    private static void aquece(MinecraftServer server) throws Exception {
        ServerLevel level = server.overworld();
        if (level.getBlockEntity(new BlockPos(4, Y + 1, 6)) instanceof mekanism.generators.common.tile.TileEntityHeatGenerator generator) {
            mekanism.common.capabilities.heat.BasicHeatCapacitor capacitor =
                  (mekanism.common.capabilities.heat.BasicHeatCapacitor) campo(mekanism.generators.common.tile.TileEntityHeatGenerator.class, "heatCapacitor", generator);
            capacitor.setHeat(4000.0, null);
        }
        run(server, String.format(Locale.ROOT, "item replace block 1 %d 8 container.0 with minecraft:cobblestone 64", Y + 2));
    }

    private static void montar() {
        // 1. O mundo A, criado do título.
        passos.add(cria(A, 20261113L));
        passos.add(zera());
        passos.add(depoisDeEntrar(40));
        passos.add(faz(() -> servidorFaz(CenaReset::config)));
        passos.add(faz(() -> MekanismAPI.debug = true));
        passos.add(faz(() -> servidorFaz(CenaReset::constroiA)));
        passos.add(zera());
        passos.add(depoisDeEntrar(60));
        passos.add(faz(() -> servidorFaz(server -> {
            enche(server);
            enchePorReflexao(server, "FluidNetwork", "", 1500);
            enchePorReflexao(server, "ChemicalNetwork", "mekanism:hydrogen", 2500);
            aquece(server);
        })));
        passos.add(zera());
        passos.add(depoisDeEntrar(60));
        passos.add(snapPasso("a1_antes_de_sair"));
        // 2. Sai para o menu e volta ao mesmo mundo.
        passos.add(sai("a1_sai"));
        passos.add(noMenu());
        passos.add(zera());
        passos.add(espera(1));
        passos.add(snapPasso("a2_menu_t1"));
        passos.add(zera());
        passos.add(espera(40));
        passos.add(snapPasso("a2_menu_t40"));
        passos.add(zera());
        passos.add(abre(A));
        passos.add(zera());
        for (int t : new int[]{1, 10, 40, 100}) {
            final int agora = t;
            passos.add(depoisDeEntrar(agora));
            passos.add(snapPasso("a3_reaberto_t" + agora));
        }
        // 3. Um cabo novo na ponta da linha de energia: a rede refeita se une.
        passos.add(faz(() -> servidorFaz(server -> run(server, String.format(Locale.ROOT, "setblock 5 %d 0 mekanism:basic_universal_cable", Y)))));
        passos.add(zera());
        passos.add(depoisDeEntrar(20));
        passos.add(snapPasso("a3_cabo_novo"));
        // 4. Outra dimensão: um cabo no Nether (chunk forçado), o jogador vai e volta.
        passos.add(faz(() -> servidorFaz(server -> {
            run(server, "execute in minecraft:the_nether run forceload add 0 0 0 0");
        })));
        passos.add(zera());
        passos.add(depoisDeEntrar(40));
        passos.add(faz(() -> servidorFaz(server -> {
            for (int x = 0; x < 3; x++) {
                run(server, String.format(Locale.ROOT, "execute in minecraft:the_nether run setblock %d 70 0 mekanism:basic_universal_cable", x));
            }
        })));
        passos.add(zera());
        passos.add(depoisDeEntrar(40));
        passos.add(faz(() -> servidorFaz(server -> {
            for (DynamicNetwork<?, ?, ?> network : nets()) {
                if (network instanceof EnergyNetwork energy && energy.transmittersSize() == 3) {
                    energy.energyContainer.setEnergy(555, null);
                }
            }
        })));
        passos.add(zera());
        passos.add(depoisDeEntrar(20));
        passos.add(snapPasso("a4_nether_cabo"));
        passos.add(faz(() -> { obsTag = "no_nether"; obsK = 0; obs = 80; }));
        passos.add(faz(() -> servidorFaz(server -> {
            run(server, "execute in minecraft:the_nether run tp @a 0.5 100 0.5");
            marca("tp_nether", server);
        })));
        passos.add(zera());
        passos.add(depoisDeEntrar(60));
        passos.add(snapPasso("a4_no_nether"));
        passos.add(faz(() -> { obsTag = "de_volta"; obsK = 0; obs = 80; }));
        passos.add(faz(() -> servidorFaz(server -> {
            run(server, "execute in minecraft:overworld run tp @a 2.5 -60 -6");
            marca("tp_overworld", server);
        })));
        passos.add(zera());
        passos.add(depoisDeEntrar(60));
        passos.add(snapPasso("a4_de_volta"));
        // 5. Sai com o Nether carregado e volta.
        passos.add(sai("a4_sai"));
        passos.add(noMenu());
        passos.add(zera());
        passos.add(espera(20));
        passos.add(snapPasso("a5_menu"));
        // 6. Um segundo mundo, diferente.
        passos.add(zera());
        passos.add(cria(B, 20261114L));
        passos.add(zera());
        passos.add(depoisDeEntrar(40));
        passos.add(faz(() -> servidorFaz(CenaReset::config)));
        passos.add(faz(() -> servidorFaz(CenaReset::constroiB)));
        passos.add(zera());
        passos.add(depoisDeEntrar(60));
        passos.add(faz(() -> servidorFaz(server -> {
            for (DynamicNetwork<?, ?, ?> network : nets()) {
                if (network instanceof EnergyNetwork energy) {
                    energy.energyContainer.setEnergy(777, null);
                }
            }
        })));
        passos.add(zera());
        passos.add(depoisDeEntrar(10));
        passos.add(snapPasso("b1_novo"));
        passos.add(sai("b1_sai"));
        passos.add(noMenu());
        passos.add(zera());
        passos.add(espera(20));
        passos.add(snapPasso("b2_menu"));
        // 7. De volta ao A (com o Nether carregado), depois ao B.
        passos.add(zera());
        passos.add(abre(A));
        passos.add(zera());
        for (int t : new int[]{1, 40, 120}) {
            final int agora = t;
            passos.add(depoisDeEntrar(agora));
            passos.add(snapPasso("a6_reaberto_t" + agora));
        }
        passos.add(sai("a6_sai"));
        passos.add(noMenu());
        passos.add(zera());
        passos.add(espera(20));
        passos.add(zera());
        passos.add(abre(B));
        passos.add(zera());
        passos.add(depoisDeEntrar(60));
        passos.add(snapPasso("b3_reaberto_t60"));
        passos.add(sai("b3_sai"));
        passos.add(noMenu());
        passos.add(zera());
        passos.add(espera(20));
        passos.add(snapPasso("fim_menu"));
        passos.add(faz(() -> {
            escreve("{\"e\":\"fim\"}");
            done = true;
            Minecraft.getInstance().stop();
        }));
    }

    @SuppressWarnings("unchecked")
    private static Set<DynamicNetwork<?, ?, ?>> nets() {
        try {
            return (Set<DynamicNetwork<?, ?, ?>>) campo(TransmitterNetworkRegistry.class, "networks", null);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        if (done || !Files.exists(ATIVA)) {
            return;
        }
        Minecraft minecraft = Minecraft.getInstance();
        if (!montado) {
            montado = true;
            try {
                Files.deleteIfExists(SAIDA);
            } catch (IOException e) {
                Mekanism.logger.error("cena-reset: limpar", e);
            }
            for (SoundSource source : SoundSource.values()) {
                minecraft.options.getSoundSourceOptionInstance(source).set(0.0);
            }
            montar();
        }
        if (indice >= passos.size()) {
            return;
        }
        try {
            if (passos.get(indice).getAsBoolean()) {
                indice++;
                n = 0;
            }
        } catch (RuntimeException e) {
            escreve("{\"e\":\"erro\",\"passo\":" + indice + ",\"erro\":" + q(String.valueOf(e)) + "}");
            Mekanism.logger.error("cena-reset: passo " + indice, e);
            indice++;
            n = 0;
        }
    }
}
