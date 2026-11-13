package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
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
import net.minecraft.core.Direction;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.chat.Component;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.Difficulty;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.ItemStack;
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
import net.neoforged.neoforge.event.server.ServerStoppedEvent;
import net.neoforged.neoforge.event.server.ServerStoppingEvent;
import net.neoforged.neoforge.transfer.ResourceHandler;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;

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

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public static void onStopping(ServerStoppingEvent event) {
        if (!Files.exists(ATIVA)) {
            return;
        }
        descargas = 0;
        escreve("{\"e\":\"stopping\",\"rotulo\":" + q(rotulo) + ",\"registro\":" + registro() + ",\"transito\":" + transito() + "}");
    }

    @SubscribeEvent
    public static void onChunkUnload(ChunkEvent.Unload event) {
        if (Files.exists(ATIVA) && !event.getLevel().isClientSide()) {
            descargas++;
        }
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
            if (minecraft.level == null && minecraft.screen instanceof TitleScreen && ++n == 60) {
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
            if (minecraft.level == null && minecraft.screen instanceof TitleScreen && ++n == 20) {
                escreve("{\"e\":\"abrindo\",\"nome\":" + q(nome) + "}");
                minecraft.createWorldOpenFlows().openWorld(nome, () -> minecraft.setScreen(new TitleScreen()));
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
                escreve("{\"e\":\"saindo\",\"rotulo\":" + q(nome) + "}");
                // O que o `PauseScreen` faz no botão de sair do mundo local.
                minecraft.level.disconnect(Component.translatable("menu.disconnect"));
                minecraft.disconnectWithSavingScreen();
                minecraft.setScreen(new TitleScreen());
            }
            return true;
        };
    }

    private static BooleanSupplier noMenu() {
        return () -> Minecraft.getInstance().level == null && Minecraft.getInstance().getSingleplayerServer() == null && Minecraft.getInstance().screen instanceof TitleScreen;
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

    private static void aquece(MinecraftServer server) {
        ServerLevel level = server.overworld();
        for (int x = 0; x <= 4; x++) {
            if (level.getBlockEntity(new BlockPos(x, Y, 6)) instanceof TileEntityTransmitter tile && tile.getTransmitter() instanceof ThermodynamicConductor conductor) {
                conductor.buffer.setHeat(900 + x, null);
            }
        }
    }

    private static void insereItens(MinecraftServer server) {
        ServerLevel level = server.overworld();
        ItemStack[] stacks = {new ItemStack(Items.COBBLESTONE, 64), new ItemStack(Items.DIRT, 32), new ItemStack(Items.SAND, 16)};
        for (ItemStack stack : stacks) {
            ResourceHandler<ItemResource> handler = level.getCapability(mekanism.common.capabilities.Capabilities.ITEM.block(), new BlockPos(1, Y, 8), Direction.NORTH);
            int inserted = -1;
            if (handler != null) {
                try (Transaction transaction = Transaction.openRoot()) {
                    inserted = handler.insert(ItemResource.of(stack), stack.getCount(), transaction);
                    transaction.commit();
                }
            }
            escreve("{\"e\":\"insere\",\"item\":" + q(String.valueOf(stack.getItem())) + ",\"inseriu\":" + inserted + "}");
        }
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
            insereItens(server);
        })));
        passos.add(zera());
        passos.add(depoisDeEntrar(8));
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
        passos.add(faz(() -> servidorFaz(server -> run(server, "execute in minecraft:the_nether run tp @a 0.5 100 0.5"))));
        passos.add(zera());
        passos.add(depoisDeEntrar(60));
        passos.add(snapPasso("a4_no_nether"));
        passos.add(faz(() -> servidorFaz(server -> run(server, "execute in minecraft:overworld run tp @a 2.5 -60 -6"))));
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
