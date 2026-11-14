package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Set;
import java.util.concurrent.TimeUnit;
import java.util.function.BooleanSupplier;
import mekanism.api.MekanismAPI;
import mekanism.common.Mekanism;
import mekanism.common.capabilities.energy.BasicEnergyContainer;
import mekanism.common.content.network.EnergyNetwork;
import mekanism.common.lib.transmitter.DynamicBufferedResourceNetwork;
import mekanism.common.lib.transmitter.DynamicNetwork;
import mekanism.common.lib.transmitter.TransmitterNetworkRegistry;
import mekanism.api.RelativeSide;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.tile.interfaces.ISideConfiguration;
import mekanism.common.tile.base.TileEntityMekanism;
import net.minecraft.client.CameraType;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.client.gui.screens.TitleScreen;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.Container;
import net.minecraft.world.Difficulty;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.LevelSettings;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.ImposterProtoChunk;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.server.level.ChunkHolder;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.minecraft.world.level.levelgen.WorldOptions;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.event.server.ServerStartedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;

/// Os setups reais em jogo (frente `setups-reais`, decisão 196): linhas longas de cabo, cano, tubo, condutor e transportador, geradores, cubos de energia, tanques e baús, montados por
/// comando (`cenas/setups_reais.txt`, o mesmo arquivo do port) num mundo superplano; o mundo sai para o menu e volta, o jogador se afasta e volta. A cada tick das janelas de observação
/// (a abertura, o fluxo, o afastar e o voltar) grava uma linha de `setups-reais.jsonl`: os chunks carregados da fila (cx, 0) (`T` andando, `L` carregado e parado, `.` descarregado), as redes
/// do registro (tipo, transmissores, aceitadores, buffer, capacidade e o que saiu no tick), os cubos de energia, os tanques, o calor e os baús de destino. Só age com o arquivo `cena-setups`
/// ao lado (o conteúdo do `setups_reais.txt` copiado pelo `oficial_setups.sh`).
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaSetups {

    private static final Path ATIVA = Path.of("cena-setups");
    private static final Path SAIDA = Path.of("setups-reais.jsonl");
    private static final String A = "SetupsA";
    private static final int Y = -60;
    private static final int[] CUBOS = {20, 100, 200, 232, 300};
    private static final int[] TANQUES = {100, 190};
    private static final int FILA_DE = -2;
    private static final int FILA_ATE = 30;

    private static final List<BooleanSupplier> passos = new ArrayList<>();
    private static boolean montado;
    private static boolean done;
    private static int indice;
    private static int n;
    private static int entrou;
    private static String rotulo = "";
    /// A janela de observação: o rótulo, os ticks que faltam e o tick dentro dela. Armada pelo passo `observa` (vale no tick seguinte) ou pelo `ServerStartedEvent` (`armarAoAbrir`).
    private static String obsTag = "";
    private static int obs;
    private static int obsK;
    private static String armarAoAbrir = "";
    private static int armarAoAbrirTicks;

    private static void escreve(String linha) {
        try {
            Files.writeString(SAIDA, linha + "\n", StandardOpenOption.CREATE, StandardOpenOption.APPEND);
        } catch (IOException e) {
            Mekanism.logger.error("cena-setups: escrita", e);
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

    /// O campo de uma classe ou de uma superclasse dela.
    private static Object campoAcima(Object target, String name) {
        for (Class<?> type = target.getClass(); type != null; type = type.getSuperclass()) {
            try {
                return campo(type, name, target);
            } catch (ReflectiveOperationException e) {
                // sobe um nível
            }
        }
        return null;
    }

    private static Object metodoAcima(Object target, String name) {
        for (Class<?> type = target.getClass(); type != null; type = type.getSuperclass()) {
            try {
                Method method = type.getDeclaredMethod(name);
                method.setAccessible(true);
                return method.invoke(target);
            } catch (ReflectiveOperationException e) {
                // sobe um nível
            }
        }
        return null;
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // O estado
    // ---------------------------------------------------------------------------------------------------------------------

    @SuppressWarnings("unchecked")
    private static Set<DynamicNetwork<?, ?, ?>> redes() {
        try {
            return (Set<DynamicNetwork<?, ?, ?>>) campo(TransmitterNetworkRegistry.class, "networks", null);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    private static String rede(DynamicNetwork<?, ?, ?> network) {
        String tipo = network.getClass().getSimpleName().replace("Network", "");
        StringBuilder sb = new StringBuilder("{\"k\":").append(q(tipo)).append(",\"tx\":").append(network.transmittersSize()).append(",\"acc\":").append(network.getAcceptorCount());
        Object capacidade = metodoAcima(network, "getCapacity");
        if (network instanceof EnergyNetwork energy) {
            sb.append(",\"buf\":").append(energy.energyContainer.getAmountAsLong());
        } else if (network instanceof DynamicBufferedResourceNetwork<?, ?, ?, ?> resource) {
            long quantidade = -1;
            try {
                Object container = campo(DynamicBufferedResourceNetwork.class, "container", resource);
                quantidade = (Long) Class.forName("mekanism.api.resource.IResourceContainer").getMethod("amountAsLong").invoke(container);
            } catch (ReflectiveOperationException | RuntimeException e) {
                // fica -1
            }
            sb.append(",\"buf\":").append(quantidade);
        } else if (tipo.equals("Heat")) {
            sb.append(",\"temp\":").append(campoAcima(network, "meanTemp")).append(",\"perda\":").append(campoAcima(network, "heatLost")).append(",\"troca\":").append(campoAcima(network, "heatTransferred"));
        }
        if (capacidade != null) {
            sb.append(",\"cap\":").append(capacidade);
        }
        Object prev = campoAcima(network, "prevTransferAmount");
        if (prev != null) {
            sb.append(",\"saiu\":").append(prev);
        }
        return sb.append('}').toString();
    }

    private static String redesJson() {
        List<DynamicNetwork<?, ?, ?>> lista = new ArrayList<>(redes());
        // O registro é um conjunto de ordem sorteada: ordena por tipo e tamanho para a comparação.
        lista.sort(Comparator.comparing((DynamicNetwork<?, ?, ?> net) -> net.getClass().getSimpleName()).thenComparing(net -> -net.transmittersSize()));
        StringBuilder sb = new StringBuilder("[");
        boolean first = true;
        for (DynamicNetwork<?, ?, ?> network : lista) {
            sb.append(first ? "" : ",").append(rede(network));
            first = false;
        }
        return sb.append(']').toString();
    }

    /// Os chunks inteiros (`hasChunk`) numa janela de 81 por 81 em volta da origem: o que o port conta em `world.chunks`, e não os `ChunkHolder` de todo estado que o `getLoadedChunksCount` conta.
    private static int cheios(ServerLevel level) {
        int total = 0;
        for (int cx = -40; cx <= 40; cx++) {
            for (int cz = -40; cz <= 40; cz++) {
                if (level.getChunkSource().hasChunk(cx, cz)) {
                    total++;
                }
            }
        }
        return total;
    }

    private static String fila(ServerLevel level) {
        StringBuilder sb = new StringBuilder();
        for (int cx = FILA_DE; cx <= FILA_ATE; cx++) {
            boolean carregado = level.getChunkSource().hasChunk(cx, 0);
            boolean andando = level.getChunkSource().isPositionTicking(ChunkPos.pack(cx, 0));
            sb.append(carregado ? (andando ? 'T' : 'L') : '.');
        }
        return sb.toString();
    }

    /// A memória do servidor por chunk da linha (decisão 199): de cada (cx, 0) de 0 a `NIVEIS_ATE`, `[cx, nível do ticket, tipo do chunk que o `ChunkHolder` guarda, block entities, removidos]`.
    /// O tipo é `L` para um `LevelChunk`, `I` para o `ImposterProtoChunk` que o embrulha, `P<n>` para um `ProtoChunk` no estado de índice `n` e `-` sem chunk; sem holder, o chunk não está na tabela.
    private static final int NIVEIS_ATE = 36;

    private static String niveis(ServerLevel level) {
        StringBuilder sb = new StringBuilder("[");
        boolean primeiro = true;
        for (int cx = 0; cx <= NIVEIS_ATE; cx++) {
            ChunkHolder holder = level.getChunkSource().chunkMap.getUpdatingChunkIfPresent(ChunkPos.pack(cx, 0));
            if (holder == null) {
                continue;
            }
            ChunkAccess chunk = holder.getLatestChunk();
            String tipo = "-";
            int bes = 0;
            int removidos = 0;
            LevelChunk nivel = null;
            if (chunk instanceof LevelChunk lc) {
                tipo = "L";
                nivel = lc;
            } else if (chunk instanceof ImposterProtoChunk imposter) {
                tipo = "I";
                nivel = imposter.getWrapped();
            } else if (chunk != null) {
                tipo = "P" + chunk.getPersistedStatus().getIndex();
            }
            if (nivel != null) {
                for (BlockEntity be : nivel.getBlockEntities().values()) {
                    bes++;
                    if (be.isRemoved()) {
                        removidos++;
                    }
                }
            }
            sb.append(primeiro ? "" : ",").append('[').append(cx).append(',').append(holder.getTicketLevel()).append(',').append(q(tipo)).append(',').append(bes).append(',').append(removidos).append(']');
            primeiro = false;
        }
        return sb.append(']').toString();
    }

    /// Os `LevelChunk` que os `ChunkHolder` guardam numa janela de 141 por 141 chunks em volta da origem (decisão 199): `todos` os que existem (nível 33 ou menos, e os de 34 a 44 que o original segura) e os que têm nível acima de 33.
    private static String memoria(ServerLevel level) {
        int todos = 0;
        int acima = 0;
        for (int cx = -70; cx <= 70; cx++) {
            for (int cz = -70; cz <= 70; cz++) {
                ChunkHolder holder = level.getChunkSource().chunkMap.getUpdatingChunkIfPresent(ChunkPos.pack(cx, cz));
                if (holder == null) {
                    continue;
                }
                ChunkAccess chunk = holder.getLatestChunk();
                if (chunk instanceof LevelChunk || chunk instanceof ImposterProtoChunk) {
                    todos++;
                    if (holder.getTicketLevel() > 33) {
                        acima++;
                    }
                }
            }
        }
        return "{\"lc\":" + todos + ",\"lc_acima33\":" + acima + "}";
    }

    @SubscribeEvent
    public static void onChunkLoad(ChunkEvent.Load event) {
        if (obs > 0 && Files.exists(ATIVA) && !event.getLevel().isClientSide()) {
            escreve("{\"e\":\"chunk\",\"tag\":" + q(obsTag) + ",\"k\":" + (obsK + 1) + ",\"ev\":\"load\",\"cx\":" + event.getChunk().getPos().x() + ",\"cz\":" + event.getChunk().getPos().z() + "}");
        }
    }

    @SubscribeEvent
    public static void onChunkUnload(ChunkEvent.Unload event) {
        if (obs > 0 && Files.exists(ATIVA) && !event.getLevel().isClientSide()) {
            escreve("{\"e\":\"chunk\",\"tag\":" + q(obsTag) + ",\"k\":" + (obsK + 1) + ",\"ev\":\"unload\",\"cx\":" + event.getChunk().getPos().x() + ",\"cz\":" + event.getChunk().getPos().z() + "}");
        }
    }

    private static long energia(ServerLevel level, BlockPos pos) {
        if (!level.hasChunkAt(pos)) {
            return -1;
        }
        if (level.getBlockEntity(pos) instanceof TileEntityMekanism tile && tile.getEnergyContainer() instanceof BasicEnergyContainer container) {
            return container.getAmountAsLong();
        }
        return -2;
    }

    private static long nosBaus(ServerLevel level, BlockPos pos) {
        if (!level.hasChunkAt(pos)) {
            return -1;
        }
        if (level.getBlockEntity(pos) instanceof Container container) {
            long total = 0;
            for (int i = 0; i < container.getContainerSize(); i++) {
                total += container.getItem(i).getCount();
            }
            return total;
        }
        return -2;
    }

    private static String tanques(ServerLevel level) {
        StringBuilder sb = new StringBuilder("{\"lava\":[");
        for (int i = 0; i < TANQUES.length; i++) {
            BlockPos pos = new BlockPos(TANQUES[i], Y, 7);
            long valor = -1;
            if (level.hasChunkAt(pos)) {
                valor = level.getBlockEntity(pos) instanceof mekanism.generators.common.tile.TileEntityHeatGenerator generator ? generator.lavaTank.amountAsLong() : -2;
            }
            sb.append(i == 0 ? "" : ",").append(valor);
        }
        sb.append("],\"carbono\":[");
        for (int i = 0; i < TANQUES.length; i++) {
            BlockPos pos = new BlockPos(TANQUES[i], Y, 13);
            long valor = -1;
            if (level.hasChunkAt(pos)) {
                valor = level.getBlockEntity(pos) instanceof mekanism.common.tile.machine.TileEntityMetallurgicInfuser infuser ? infuser.infusionTank.amountAsLong() : -2;
            }
            sb.append(i == 0 ? "" : ",").append(valor);
        }
        sb.append("],\"calor\":[");
        for (int i = -1; i < TANQUES.length; i++) {
            BlockPos pos = new BlockPos(i < 0 ? 0 : TANQUES[i], Y, i < 0 ? 18 : 19);
            String valor = "null";
            if (level.hasChunkAt(pos)) {
                valor = "-2";
                if (level.getBlockEntity(pos) instanceof mekanism.generators.common.tile.TileEntityHeatGenerator generator) {
                    try {
                        Object capacitor = campo(mekanism.generators.common.tile.TileEntityHeatGenerator.class, "heatCapacitor", generator);
                        valor = String.valueOf(((mekanism.common.capabilities.heat.BasicHeatCapacitor) capacitor).getHeat());
                    } catch (ReflectiveOperationException | RuntimeException e) {
                        valor = "-3";
                    }
                }
            }
            sb.append(i < 0 ? "" : ",").append(valor);
        }
        return sb.append("]}").toString();
    }

    /// As pilhas em trânsito da pista de transportadores (x de 1 a 159, z = 24): a soma do `transit` de cada um.
    private static String transito(ServerLevel level) {
        int total = 0;
        for (int x = 1; x <= 159; x++) {
            BlockPos pos = new BlockPos(x, Y, 24);
            if (!level.hasChunkAt(pos)) {
                continue;
            }
            if (level.getBlockEntity(pos) instanceof mekanism.common.tile.transmitter.TileEntityTransmitter tile
                  && tile.getTransmitter() instanceof mekanism.common.content.network.transmitter.LogisticalTransporterBase transporter) {
                try {
                    total += ((Map<?, ?>) campo(mekanism.common.content.network.transmitter.LogisticalTransporterBase.class, "transit", transporter)).size();
                } catch (ReflectiveOperationException | RuntimeException e) {
                    return "-1";
                }
            }
        }
        return String.valueOf(total);
    }

    private static String estado(MinecraftServer server) {
        ServerLevel level = server.overworld();
        StringBuilder sb = new StringBuilder();
        sb.append("\"memoria\":").append(memoria(level)).append(",\"niveis\":").append(niveis(level)).append(",\"carregados\":").append(level.getChunkSource().getLoadedChunksCount()).append(",\"cheios\":").append(cheios(level)).append(",\"fila\":").append(q(fila(level)));
        sb.append(",\"redes\":").append(redesJson());
        sb.append(",\"gerador\":").append(energia(level, new BlockPos(0, Y, 0)));
        sb.append(",\"cubos\":[");
        for (int i = 0; i < CUBOS.length; i++) {
            sb.append(i == 0 ? "" : ",").append(energia(level, new BlockPos(CUBOS[i], Y, 1)));
        }
        sb.append("],\"tanques\":").append(tanques(level));
        sb.append(",\"transito\":").append(transito(level));
        sb.append(",\"baus\":[").append(nosBaus(level, new BlockPos(1, Y + 2, 24))).append(',').append(nosBaus(level, new BlockPos(140, Y, 25))).append(']');
        return sb.toString();
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // As janelas de observação
    // ---------------------------------------------------------------------------------------------------------------------

    @SubscribeEvent
    public static void onStarted(ServerStartedEvent event) {
        if (Files.exists(ATIVA) && !armarAoAbrir.isEmpty()) {
            obsTag = armarAoAbrir;
            obs = armarAoAbrirTicks;
            obsK = 0;
            armarAoAbrir = "";
            escreve("{\"e\":\"started\",\"tag\":" + q(obsTag) + ",\"ms\":" + System.currentTimeMillis() + "}");
        }
    }

    @SubscribeEvent(priority = EventPriority.HIGHEST)
    public static void onServerTickPre(ServerTickEvent.Pre event) {
        if (obs > 0 && Files.exists(ATIVA)) {
            ServerLevel level = event.getServer().overworld();
            escreve("{\"e\":\"pre\",\"tag\":" + q(obsTag) + ",\"k\":" + (obsK + 1) + ",\"jogadores\":" + event.getServer().getPlayerCount() + ",\"carregados\":" + level.getChunkSource().getLoadedChunksCount()
                  + ",\"cheios\":" + cheios(level) + ",\"fila\":" + q(fila(level)) + "}");
        }
    }

    @SubscribeEvent(priority = EventPriority.LOWEST)
    public static void onServerTick(ServerTickEvent.Post event) {
        if (obs > 0 && Files.exists(ATIVA)) {
            obs--;
            obsK++;
            escreve("{\"e\":\"tick\",\"tag\":" + q(obsTag) + ",\"k\":" + obsK + ",\"ms\":" + System.currentTimeMillis() + "," + estado(event.getServer()) + "}");
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

    private static BooleanSupplier cria(String nome, long semente) {
        return () -> {
            Minecraft minecraft = Minecraft.getInstance();
            if (noMundo(minecraft)) {
                return true;
            }
            if (minecraft.level == null && minecraft.getSingleplayerServer() == null && ++n == 60) {
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

    private static BooleanSupplier abre(String nome, String janela, int ticks) {
        return () -> {
            Minecraft minecraft = Minecraft.getInstance();
            if (noMundo(minecraft)) {
                return true;
            }
            if (minecraft.level == null && minecraft.getSingleplayerServer() == null && ++n == 20) {
                armarAoAbrir = janela;
                armarAoAbrirTicks = ticks;
                escreve("{\"e\":\"abrindo\",\"nome\":" + q(nome) + ",\"tag\":" + q(janela) + ",\"ms\":" + System.currentTimeMillis() + "}");
                minecraft.createWorldOpenFlows().openWorld(nome, () -> minecraft.setScreenAndShow(new TitleScreen()));
            }
            return false;
        };
    }

    /// Conta os ticks do cliente com o jogador no mundo: o passo seguinte só roda depois de `ticks` deles.
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

    private interface Acao {
        void accept(MinecraftServer server) throws Exception;
    }

    private static void servidorFaz(Acao action) {
        MinecraftServer server = Minecraft.getInstance().getSingleplayerServer();
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
            }).get(60, TimeUnit.SECONDS);
        } catch (Exception e) {
            escreve("{\"e\":\"erro\",\"onde\":\"servidorFaz\",\"erro\":" + q(String.valueOf(e)) + "}");
        }
    }

    private static BooleanSupplier observa(String tag, int ticks) {
        return faz(() -> {
            obsTag = tag;
            obs = ticks;
            obsK = 0;
            escreve("{\"e\":\"janela\",\"tag\":" + q(tag) + ",\"ticks\":" + ticks + ",\"ms\":" + System.currentTimeMillis() + "}");
        });
    }

    private static BooleanSupplier marca(String tag) {
        return faz(() -> servidorFaz(server -> escreve("{\"e\":\"marca\",\"tag\":" + q(tag) + ",\"ms\":" + System.currentTimeMillis() + "," + estado(server) + "}")));
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // Os passos nomeados do arquivo (`@passo`): o que o port faz pelo NBT, o original faz pelo Java
    // ---------------------------------------------------------------------------------------------------------------------

    @SuppressWarnings("unchecked")
    private static void enche(String familia, Object recurso, long quantidade) throws Exception {
        for (DynamicNetwork<?, ?, ?> network : new ArrayList<>(redes())) {
            if (!network.getClass().getSimpleName().equals(familia)) {
                continue;
            }
            Object container = campo(DynamicBufferedResourceNetwork.class, "container", network);
            for (Method method : container.getClass().getMethods()) {
                if (method.getName().equals("setContents") && method.getParameterCount() == 3 && method.getParameterTypes()[1] == long.class) {
                    method.invoke(container, recurso, quantidade, null);
                    break;
                }
            }
        }
    }

    /// O que a colocação por item dá a uma máquina posta por `setblock` (decisão 113): os componentes do item e `INPUT` nos seis lados do tipo `type`, com a capability de cada lado reavaliada.
    private static void configura(ServerLevel level, BlockPos pos, TransmissionType type) {
        if (level.getBlockEntity(pos) instanceof TileEntityMekanism tile && tile instanceof ISideConfiguration sides) {
            tile.applyComponentsFromItemStack(new ItemStack(level.getBlockState(pos).getBlock().asItem()));
            ConfigInfo info = sides.getConfig().getConfig(type);
            for (RelativeSide relative : RelativeSide.VALUES) {
                info.setDataType(DataType.INPUT, relative);
                sides.getConfig().sideChanged(type, relative);
            }
        }
    }

    private static void passoNomeado(MinecraftServer server, String nome) throws Exception {
        ServerLevel level = server.overworld();
        switch (nome) {
            case "@combustivel" -> {
                if (level.getBlockEntity(new BlockPos(0, Y, 0)) instanceof TileEntityMekanism generator) {
                    var coal = BuiltInRegistries.ITEM.getValue(Identifier.parse("minecraft:coal"));
                    generator.getInventorySlots().get(0).setContents(ItemResource.of(coal), 64, null);
                }
            }
            case "@maquinas" -> {
                for (int x : CUBOS) {
                    configura(level, new BlockPos(x, Y, 1), TransmissionType.ENERGY);
                }
                for (int x : TANQUES) {
                    configura(level, new BlockPos(x, Y, 13), TransmissionType.CHEMICAL);
                }
            }
            case "@minerio" -> {
                var ore = BuiltInRegistries.ITEM.getValue(Identifier.parse("minecraft:iron_ore"));
                for (int x : CUBOS) {
                    if (level.getBlockEntity(new BlockPos(x, Y, 1)) instanceof TileEntityMekanism machine) {
                        machine.getInventorySlots().get(0).setContents(ItemResource.of(ore), 64, null);
                    }
                }
            }
            case "@lava" -> enche("FluidNetwork", FluidResource.of(net.minecraft.world.level.material.Fluids.LAVA), 20_000);
            case "@carbono" -> {
                var key = net.minecraft.resources.ResourceKey.create(mekanism.api.MekanismRegistries.Keys.CHEMICAL, Identifier.parse("mekanism:carbon"));
                enche("ChemicalNetwork", mekanism.api.chemical.ChemicalResource.of(server.registryAccess().getOrThrow(key)), 5_000);
            }
            case "@calor" -> {
                if (level.getBlockEntity(new BlockPos(0, Y, 18)) instanceof mekanism.generators.common.tile.TileEntityHeatGenerator generator) {
                    mekanism.common.capabilities.heat.BasicHeatCapacitor capacitor =
                          (mekanism.common.capabilities.heat.BasicHeatCapacitor) campo(mekanism.generators.common.tile.TileEntityHeatGenerator.class, "heatCapacitor", generator);
                    capacitor.setHeat(30000.0, null);
                }
            }
            case "@itens" -> run(server, String.format(Locale.ROOT, "item replace block 1 %d 24 container.0 with minecraft:cobblestone 64", Y + 2));
            default -> throw new IllegalArgumentException(nome);
        }
    }

    // ---------------------------------------------------------------------------------------------------------------------
    // O roteiro
    // ---------------------------------------------------------------------------------------------------------------------

    /// As linhas de uma seção do `cena-setups` (`== nome`), sem comentário nem linha em branco.
    private static List<String> secao(String nome) {
        List<String> linhas = new ArrayList<>();
        try {
            boolean dentro = false;
            for (String linha : Files.readAllLines(ATIVA)) {
                if (linha.startsWith("== ")) {
                    dentro = linha.substring(3).trim().equals(nome);
                } else if (dentro && !linha.isBlank() && !linha.startsWith("#")) {
                    linhas.add(linha.trim());
                }
            }
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
        return linhas;
    }

    private static void roda(String nome) {
        for (String linha : secao(nome)) {
            if (linha.startsWith("cmd ")) {
                String comando = linha.substring(4);
                passos.add(faz(() -> servidorFaz(server -> run(server, comando))));
            } else if (linha.startsWith("espera ")) {
                int ticks = Integer.parseInt(linha.substring(7).trim());
                passos.add(zera());
                passos.add(depoisDeEntrar(ticks));
            } else if (linha.startsWith("@")) {
                passos.add(faz(() -> servidorFaz(server -> passoNomeado(server, linha))));
            } else {
                throw new IllegalArgumentException(linha);
            }
        }
    }

    /// O clique do Network Reader no cabo (100, -60, 0), como o de `CenaLeitor` (`ServerPlayerGameMode.useItemOn`, pela face norte), e o print do cliente (1920 por 1080, GUI 3, o chat com a resposta).
    private static void clicaLeitor(MinecraftServer server) {
        ServerPlayer player = server.getPlayerList().getPlayers().getFirst();
        BlockPos pos = new BlockPos(100, Y, 0);
        BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(pos).relative(Direction.NORTH, 0.5), Direction.NORTH, pos, false);
        player.gameMode.useItemOn(player, player.level(), player.getItemInHand(InteractionHand.MAIN_HAND), InteractionHand.MAIN_HAND, hit);
    }

    private static BooleanSupplier print(String nome) {
        return faz(() -> {
            Minecraft minecraft = Minecraft.getInstance();
            Screenshot.grab(minecraft.gameDirectory, nome + ".png", minecraft.gameRenderer.mainRenderTarget(), 1, message -> Mekanism.logger.info("CENA-SETUPS print: {}", message.getString()));
        });
    }

    private static BooleanSupplier vistaDoLeitor() {
        return faz(() -> {
            Minecraft minecraft = Minecraft.getInstance();
            minecraft.options.setCameraType(CameraType.FIRST_PERSON);
            minecraft.options.chatVisibility().set(net.minecraft.world.entity.player.ChatVisiblity.FULL);
            minecraft.options.guiScale().set(3);
        });
    }

    private static void montar() {
        // 1. O mundo, criado do título, e a construção.
        passos.add(cria(A, 20261116L));
        passos.add(zera());
        passos.add(depoisDeEntrar(40));
        roda("construir");
        passos.add(marca("construido"));
        // 2. Sai (o mundo salva com tudo parado) e abre: a abertura, 80 ticks de cada vez.
        passos.add(sai("c1_sai"));
        passos.add(noMenu());
        passos.add(zera());
        passos.add(espera(20));
        passos.add(abre(A, "abre1", 80));
        passos.add(zera());
        passos.add(depoisDeEntrar(220));
        passos.add(marca("depois_de_abrir"));
        // 3. O fluxo: combustível, lava, carbono, calor e itens, e 600 ticks vistos um a um.
        passos.add(observa("fluxo", 600));
        roda("fluxo");
        passos.add(zera());
        passos.add(depoisDeEntrar(630));
        passos.add(marca("depois_do_fluxo"));
        // 4. Se afasta (os chunks descarregam na borda) e volta.
        passos.add(observa("afasta", 300));
        roda("afasta");
        passos.add(zera());
        passos.add(depoisDeEntrar(320));
        passos.add(marca("depois_de_afastar"));
        passos.add(observa("volta", 300));
        roda("volta");
        passos.add(zera());
        passos.add(depoisDeEntrar(320));
        passos.add(marca("depois_de_voltar"));
        // 4a. A máquina de um chunk que o original segura mas o Horizon solta (decisão 199): vai a 400 blocos, volta, e lê a energia das Enrichment Chambers antes, longe e depois.
        passos.add(marca("antes_lejos"));
        passos.add(observa("lejos", 300));
        roda("lejos_vai");
        passos.add(zera());
        passos.add(depoisDeEntrar(100));
        passos.add(marca("lejos_no_meio"));
        roda("lejos_volta");
        passos.add(zera());
        passos.add(depoisDeEntrar(150));
        passos.add(marca("depois_lejos"));
        // 4b. O Network Reader num cabo distante (x = 100, chunk 6): o print que se empilha com o do port.
        passos.add(vistaDoLeitor());
        passos.add(observa("leitor", 100));
        roda("leitor");
        passos.add(faz(() -> servidorFaz(CenaSetups::clicaLeitor)));
        passos.add(zera());
        passos.add(depoisDeEntrar(40));
        passos.add(print("setups_oficial_leitor"));
        roda("leitor_volta");
        // 5. Sai com tudo andando (energia no meio, itens em trânsito) e abre de novo.
        passos.add(sai("f_sai"));
        passos.add(noMenu());
        passos.add(zera());
        passos.add(espera(20));
        passos.add(abre(A, "abre2", 80));
        passos.add(zera());
        passos.add(depoisDeEntrar(100));
        passos.add(marca("depois_de_reabrir"));
        passos.add(sai("g_sai"));
        passos.add(noMenu());
        passos.add(zera());
        passos.add(espera(20));
        passos.add(faz(() -> {
            escreve("{\"e\":\"fim\"}");
            done = true;
            Minecraft.getInstance().stop();
        }));
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
                Mekanism.logger.error("cena-setups: limpar", e);
            }
            for (SoundSource source : SoundSource.values()) {
                minecraft.options.getSoundSourceOptionInstance(source).set(0.0);
            }
            MekanismAPI.debug = false;
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
            Mekanism.logger.error("cena-setups: passo " + indice, e);
            indice++;
            n = 0;
        }
    }
}
