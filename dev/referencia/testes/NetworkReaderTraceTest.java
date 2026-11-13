package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.api.MekanismAPI;
import mekanism.api.RelativeSide;
import mekanism.api.heat.IHeatHandler;
import mekanism.common.capabilities.Capabilities;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.MekanismRegistries;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.content.network.ChemicalNetwork;
import mekanism.common.content.network.EnergyNetwork;
import mekanism.common.content.network.FluidNetwork;
import mekanism.common.content.network.HeatNetwork;
import mekanism.common.lib.transmitter.DynamicNetwork;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.lib.transmitter.TransmitterNetworkRegistry;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismItems;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.tile.interfaces.ISideConfiguration;
import mekanism.common.tile.transmitter.TileEntityTransmitter;
import mekanism.common.util.WorldUtils;
import mekanism.generators.common.registries.GeneratorsBlocks;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientboundSystemChatPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.permissions.LevelBasedPermissionSet;
import net.minecraft.server.permissions.PermissionLevel;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.ChatVisiblity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.GameTestPlayer;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Frente `fase3-faltas`: grava o `ItemNetworkReader` pelo caminho de verdade do servidor (`ServerPlayerGameMode.useItemOn`, com um `GameTestPlayer`), nos transmissores das cinco famílias
/// (cabo, cano, tubo, condutor, transportador), nos vizinhos (uma máquina, um Heat Generator, um baú), na pedra e no ar. Cada linha tem o resultado do clique, as mensagens de chat que o jogador
/// recebeu (JSON de `Component`, na ordem) e os números crus da rede lidos por reflexão (o que o port alimenta ao texto). Os valores da rede são postos por reflexão no mesmo tick do clique.
/// Um `runGameTestServer`, o arquivo `network_reader.jsonl`.
///
/// Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar (`oficial_gametest_rastro.sh`) e apague-o depois.
@ForEachTest(groups = "trace")
public class NetworkReaderTraceTest {

    private static final int TIMEOUT = 900 * SharedConstants.TICKS_PER_SECOND;
    private static final String BATCH = "trace_network_reader";
    private static final String COMMIT = "b2d04e166";
    private static final int SETTLE = 10;
    private static final Direction[] SIDES = Direction.values();

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(9, 4, 9).fill(0, 0, 0, 8, 0, 8, Blocks.STONE.defaultBlockState()));
    }

    private static Block block(String id) {
        return BuiltInRegistries.BLOCK.getValue(Identifier.parse(id));
    }

    private static Field field(Class<?> type, String name) {
        for (Class<?> c = type; c != null; c = c.getSuperclass()) {
            try {
                Field field = c.getDeclaredField(name);
                field.setAccessible(true);
                return field;
            } catch (NoSuchFieldException ignored) {
            }
        }
        throw new IllegalStateException(name);
    }

    private static Object get(Object target, String name) {
        try {
            return field(target.getClass(), name).get(target);
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }

    private static void set(Object target, String name, Object value) {
        try {
            field(target.getClass(), name).set(target, value);
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }

    private static JsonElement component(Component component) {
        return ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, component).getOrThrow();
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static JsonArray drainChat(GameTestPlayer player) {
        JsonArray chat = new JsonArray();
        for (Packet<?> packet : (List<Packet<?>>) (List) ((java.util.stream.Stream) player.getOutboundPackets((Class) Packet.class)).toList()) {
            if (packet instanceof ClientboundSystemChatPacket system && !system.overlay()) {
                chat.add(component(system.content()));
            }
        }
        player.clearOutboundPackets();
        return chat;
    }

    private static JsonObject result(InteractionResult result) {
        JsonObject out = new JsonObject();
        out.addProperty("class", result.getClass().getSimpleName());
        out.addProperty("consumes_action", result.consumesAction());
        return out;
    }

    private static String id(FluidResource resource) {
        return resource.isEmpty() ? "" : resource.typeHolder().getRegisteredName();
    }

    private static String id(ChemicalResource resource) {
        return resource.isEmpty() ? "" : resource.typeHolder().getRegisteredName();
    }

    private static ChemicalResource chemical(String chemical) {
        int colon = chemical.indexOf(':');
        ResourceKey<Chemical> key = ResourceKey.create(MekanismRegistries.Keys.CHEMICAL, Identifier.fromNamespaceAndPath(chemical.substring(0, colon), chemical.substring(colon + 1)));
        return ChemicalResource.of(ServerLifecycleHooks.getCurrentServer().registryAccess().getOrThrow(key));
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // A cena
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final class Scene {

        final MekGameTestHelper helper;
        final List<String> lines = new ArrayList<>();
        String fileName = "network_reader.jsonl";
        String meta;
        GameTestPlayer player;

        Scene(MekGameTestHelper helper) {
            this.helper = helper;
        }

        /// O jogador de teste, criado dentro de um passo da sequência (criado antes do teste começar, a conexão dele ainda não envia pacote).
        void player() {
            if (player != null) {
                return;
            }
            this.player = (GameTestPlayer) helper.makeMockPlayerLookingAt(new BlockPos(4, 1, 4), Direction.UP);
            this.player.gameMode.changeGameModeForPlayer(GameType.SURVIVAL);
            this.player.preventItemPickup();
            // O jogador de teste nasce com o chat escondido (`ChatVisiblity.HIDDEN` deixa passar só o overlay): sem isto nenhuma mensagem de chat chega.
            set(this.player, "chatVisibility", ChatVisiblity.FULL);
            // Fora das cenas: o jogador de teste nasce na origem da estrutura e sufocaria (`inWall`) dentro de um transmissor.
            this.player.snapTo(Vec3.atBottomCenterOf(abs(new BlockPos(8, 1, 8))));
        }

        BlockPos abs(BlockPos rel) {
            return helper.absolutePos(rel);
        }

        void clear() {
            player();
            for (int x = 0; x < 9; x++) {
                for (int z = 0; z < 9; z++) {
                    for (int y = 1; y <= 3; y++) {
                        helper.setBlock(new BlockPos(x, y, z), Blocks.AIR.defaultBlockState());
                    }
                }
            }
        }

        void put(String id, BlockPos rel) {
            helper.setBlock(rel, block(id).defaultBlockState());
        }

        /// Uma Enrichment Chamber que aceita energia em todos os lados (um bloco posto por `setBlock` tem todos os lados `NONE`).
        void chamber(BlockPos rel) {
            helper.setBlock(rel, MekanismBlocks.ENRICHMENT_CHAMBER.defaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, Direction.SOUTH));
            TileEntityMekanism tile = WorldUtils.getTileEntity(TileEntityMekanism.class, helper.getLevel(), abs(rel));
            tile.applyComponentsFromItemStack(new ItemStack(MekanismBlocks.ENRICHMENT_CHAMBER.asItem()));
            ConfigInfo info = ((ISideConfiguration) tile).getConfig().getConfig(TransmissionType.ENERGY);
            for (RelativeSide relative : RelativeSide.VALUES) {
                info.setDataType(DataType.INPUT, relative);
            }
            ((MachineEnergyContainer<?>) tile.getEnergyContainer()).setMaxEnergy(1_000_000);
            tile.getEnergyContainer().setEnergy(0, null);
        }

        TileEntityTransmitter transmitter(BlockPos rel) {
            return helper.getLevel().getBlockEntity(abs(rel)) instanceof TileEntityTransmitter tile ? tile : null;
        }

        DynamicNetwork<?, ?, ?> network(BlockPos rel) {
            TileEntityTransmitter tile = transmitter(rel);
            return tile == null ? null : tile.getTransmitter().getTransmitterNetwork();
        }

        /// Os números crus da rede da qual o bloco é transmissor, no formato que o port alimenta ao texto.
        JsonObject raw(BlockPos rel, Direction face) {
            JsonObject out = new JsonObject();
            BlockEntity entity = helper.getLevel().getBlockEntity(abs(rel));
            if (entity != null) {
                IHeatHandler handler = WorldUtils.getCapability(helper.getLevel(), Capabilities.HEAT, abs(rel), null, entity, face.getOpposite());
                out.addProperty("heat_temperature", handler == null ? null : String.valueOf(handler.getTemperature()));
            }
            TileEntityTransmitter tile = transmitter(rel);
            if (tile == null) {
                out.addProperty("transmitter", false);
                return out;
            }
            out.addProperty("transmitter", true);
            DynamicNetwork<?, ?, ?> network = tile.getTransmitter().getTransmitterNetwork();
            out.addProperty("has_network", tile.getTransmitter().hasTransmitterNetwork());
            if (network == null) {
                return out;
            }
            out.addProperty("transmitters", network.transmittersSize());
            out.addProperty("acceptors", network.getAcceptorCount());
            if (network instanceof EnergyNetwork energy) {
                out.addProperty("family", "energy");
                out.addProperty("stored", String.valueOf(energy.energyContainer.getAmountAsLong()));
                out.addProperty("needed", String.valueOf(energy.energyContainer.getNeededAsLong()));
                out.addProperty("capacity", String.valueOf(energy.getCapacity()));
                out.addProperty("prev_transfer", String.valueOf(get(energy, "prevTransferAmount")));
            } else if (network instanceof FluidNetwork fluid) {
                out.addProperty("family", "fluid");
                out.addProperty("resource", id(fluid.getContainer().resource()));
                out.addProperty("stored", String.valueOf(fluid.getContainer().amountAsLong()));
                out.addProperty("needed", String.valueOf(fluid.getContainer().getNeededAsLong(FluidResource.EMPTY)));
                out.addProperty("capacity", String.valueOf(fluid.getCapacity()));
                out.addProperty("prev_transfer", String.valueOf(fluid.getPrevTransferAmount()));
            } else if (network instanceof ChemicalNetwork chemical) {
                out.addProperty("family", "chemical");
                out.addProperty("resource", id(chemical.getContainer().resource()));
                out.addProperty("stored", String.valueOf(chemical.getContainer().amountAsLong()));
                out.addProperty("needed", String.valueOf(chemical.getContainer().getNeededAsLong(ChemicalResource.EMPTY)));
                out.addProperty("capacity", String.valueOf(chemical.getCapacity()));
                out.addProperty("prev_transfer", String.valueOf(chemical.getPrevTransferAmount()));
            } else if (network instanceof HeatNetwork) {
                out.addProperty("family", "heat");
                out.addProperty("mean_temp", String.valueOf(get(network, "meanTemp")));
                out.addProperty("heat_lost", String.valueOf(get(network, "heatLost")));
                out.addProperty("heat_transferred", String.valueOf(get(network, "heatTransferred")));
            } else {
                out.addProperty("family", "inventory");
            }
            return out;
        }

        /// Cada clique é uma tarefa de um tick, e as mensagens são lidas no tick seguinte: o pacote de chat só chega à fila do jogador de teste depois que o tick do servidor o envia.
        void click(String name, BlockPos rel, Direction face, boolean sneak, java.util.function.Supplier<JsonObject> before) {
            tasks.add(new Task(1, () -> doClick(name, rel, face, sneak, before)));
        }

        private void doClick(String name, BlockPos rel, Direction face, boolean sneak, java.util.function.Supplier<JsonObject> before) {
            JsonObject extra = before == null ? null : before.get();
            ItemStack stack = MekanismItems.NETWORK_READER.asStack();
            player.setItemInHand(InteractionHand.MAIN_HAND, stack);
            player.setShiftKeyDown(sneak);
            BlockPos abs = abs(rel);
            BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(abs).relative(face, 0.5), face, abs, false);
            JsonObject line = new JsonObject();
            line.addProperty("case", name);
            line.addProperty("face", face.getSerializedName());
            line.addProperty("sneak", sneak);
            line.add("raw", raw(rel, face));
            if (extra != null) {
                line.add("set", extra);
            }
            player.clearOutboundPackets();
            InteractionResult result = null;
            try {
                result = player.gameMode.useItemOn(player, helper.getLevel(), stack, InteractionHand.MAIN_HAND, hit);
            } catch (RuntimeException ex) {
                line.addProperty("exception", ex.getClass().getSimpleName() + ": " + ex.getMessage());
            }
            player.setShiftKeyDown(false);
            if (result != null) {
                line.add("result", result(result));
            }
            pending = line;
        }


        /// As redes do registro (`TransmitterNetworkRegistry.networks`, o `ObjectOpenHashSet` estático) na ordem do iterador, com o UUID, o `hashCode` dele, a classe, as contagens e o texto de `getTextComponent`.
        JsonArray registry() {
            JsonArray out = new JsonArray();
            try {
                java.util.Set<?> networks = (java.util.Set<?>) field(TransmitterNetworkRegistry.class, "networks").get(null);
                for (Object entry : networks) {
                    DynamicNetwork<?, ?, ?> network = (DynamicNetwork<?, ?, ?>) entry;
                    JsonObject row = new JsonObject();
                    java.util.UUID uuid = (java.util.UUID) get(network, "uuid");
                    row.addProperty("class", network.getClass().getSimpleName());
                    row.addProperty("uuid_most", String.valueOf(uuid.getMostSignificantBits()));
                    row.addProperty("uuid_least", String.valueOf(uuid.getLeastSignificantBits()));
                    row.addProperty("hash", uuid.hashCode());
                    row.addProperty("transmitters", network.transmittersSize());
                    row.addProperty("acceptors", network.getAcceptorCount());
                    row.add("text", component(network.getTextComponent()));
                    out.add(row);
                }
            } catch (IllegalAccessException e) {
                throw new IllegalStateException(e);
            }
            return out;
        }

        /// A linha de `TransmitterNetworkRegistry.toComponents()`, o que o ramo de depuração do leitor escreve.
        JsonArray components() {
            JsonArray out = new JsonArray();
            for (Component component : TransmitterNetworkRegistry.toComponents()) {
                out.add(component(component));
            }
            return out;
        }

        /// As linhas de log do `Mekanism.logger` desde a última chamada (o `MekanismAPI.debug` do registro escreve com `info`).
        JsonArray logs() {
            JsonArray out = new JsonArray();
            synchronized (captured) {
                for (String line : captured) {
                    out.add(line);
                }
                captured.clear();
            }
            return out;
        }

        final List<String> captured = new ArrayList<>();

        void captureLogs() {
            org.apache.logging.log4j.core.Logger root = (org.apache.logging.log4j.core.Logger) org.apache.logging.log4j.LogManager.getRootLogger();
            org.apache.logging.log4j.core.appender.AbstractAppender appender = new org.apache.logging.log4j.core.appender.AbstractAppender("mekDebugCapture", null, null, true, org.apache.logging.log4j.core.config.Property.EMPTY_ARRAY) {
                @Override
                public void append(org.apache.logging.log4j.core.LogEvent event) {
                    if ("mekanism.common.Mekanism".equals(event.getLoggerName())) {
                        synchronized (captured) {
                            captured.add(event.getLevel().name() + " " + event.getMessage().getFormattedMessage());
                        }
                    }
                }
            };
            appender.start();
            root.addAppender(appender);
        }

        /// Um passo que só olha: grava uma linha com o que o registro tem e o que o log escreveu desde o passo anterior (cada tarefa de um tick, a linha fecha no tick seguinte junto com o chat).
        void note(String name, boolean debug) {
            tasks.add(new Task(1, () -> {
                JsonObject line = new JsonObject();
                line.addProperty("case", name);
                line.addProperty("debug", MekanismAPI.debug);
                line.add("registry", registry());
                line.add("components", components());
                line.add("log", logs());
                player.clearOutboundPackets();
                pending = line;
            }));
        }

        /// `Commands.performPrefixedCommand` com o jogador de teste como origem, no nível de permissão pedido; grava a resposta (o chat e o `MekanismAPI.debug` depois).
        void command(String name, String command, PermissionLevel level) {
            tasks.add(new Task(1, () -> {
                JsonObject line = new JsonObject();
                line.addProperty("case", name);
                line.addProperty("command", command);
                line.addProperty("level", level.name());
                line.addProperty("debug_before", MekanismAPI.debug);
                net.minecraft.commands.CommandSourceStack source = player.createCommandSourceStack().withPermission(LevelBasedPermissionSet.forLevel(level));
                com.mojang.brigadier.tree.CommandNode<net.minecraft.commands.CommandSourceStack> mek = source.getServer().getCommands().getDispatcher().getRoot().getChild("mek");
                line.addProperty("mek_usable", mek != null && mek.canUse(source));
                com.mojang.brigadier.tree.CommandNode<net.minecraft.commands.CommandSourceStack> debug = mek == null ? null : mek.getChild("debug");
                line.addProperty("debug_usable", debug != null && debug.canUse(source));
                player.clearOutboundPackets();
                source.getServer().getCommands().performPrefixedCommand(source, command);
                line.addProperty("debug_after", MekanismAPI.debug);
                pending = line;
            }));
        }

        record Task(int delay, Runnable action) {
        }

        final java.util.ArrayDeque<Task> tasks = new java.util.ArrayDeque<>();
        private JsonObject pending;
        private int waiting = -1;

        /// Um tick: fecha a linha do clique do tick anterior e roda a próxima tarefa quando o atraso dela passou. Devolve se ainda há o que fazer. As tarefas que uma tarefa cria vão para a frente da fila, na ordem.
        boolean tick() {
            if (pending != null) {
                pending.add("chat", drainChat(player));
                lines.add(pending.toString());
                pending = null;
            }
            if (tasks.isEmpty()) {
                return false;
            }
            Task next = tasks.peekFirst();
            if (waiting < 0) {
                waiting = next.delay();
            }
            if (--waiting > 0) {
                return true;
            }
            waiting = -1;
            tasks.pollFirst();
            java.util.ArrayDeque<Task> outer = new java.util.ArrayDeque<>(tasks);
            tasks.clear();
            next.action().run();
            java.util.ArrayDeque<Task> created = new java.util.ArrayDeque<>(tasks);
            tasks.clear();
            tasks.addAll(created);
            tasks.addAll(outer);
            return true;
        }

        void write() {
            List<String> all = new ArrayList<>();
            all.add(meta != null ? meta : "{\"meta\":\"commit " + COMMIT + ". The Network Reader (ItemNetworkReader.useOn) clicked through ServerPlayerGameMode.useItemOn with a GameTestPlayer. case: scene/target/detail. face: the clicked face. sneak: the "
                  + "player sneaking. set: the values put by reflection in the same tick, right before the click (energy: stored and prev_transfer; fluid and chemical: resource, stored; heat: mean_temp, heat_lost, heat_transferred). "
                  + "raw: the numbers of the network the clicked block transmits for (family, transmitters, acceptors, stored, needed, capacity, prev_transfer or the heat fields; resource ids; longs as strings; doubles as Double.toString). "
                  + "result: InteractionResult of useItemOn. chat: the non overlay system messages the player received, ComponentSerialization.CODEC JSON, in order.\"}");
            all.addAll(lines);
            try {
                Path file = Path.of("trace", fileName);
                Files.createDirectories(file.getParent());
                Files.write(file, all);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }

    private static final BlockPos A = new BlockPos(1, 1, 1);
    private static final BlockPos B = new BlockPos(2, 1, 1);
    private static final BlockPos C = new BlockPos(3, 1, 1);
    private static final BlockPos END = new BlockPos(4, 1, 1);
    private static final BlockPos SIDE = new BlockPos(2, 1, 2);

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the Network Reader on every family of transmitter, on their neighbours, on stone and on air.")
    public static void networkReader(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> {
            Scene scene = new Scene(helper);
            var real = helper.startSequence();
            java.util.function.BiConsumer<Integer, Runnable> sequence = (ticks, action) -> scene.tasks.add(new Scene.Task(ticks, action));
            energy(scene, sequence, "cable_basic", "mekanism:basic_universal_cable");
            energy(scene, sequence, "cable_ultimate", "mekanism:ultimate_universal_cable");
            fluid(scene, sequence, "pipe_basic", "mekanism:basic_mechanical_pipe");
            fluid(scene, sequence, "pipe_ultimate", "mekanism:ultimate_mechanical_pipe");
            chemical(scene, sequence, "tube_basic", "mekanism:basic_pressurized_tube");
            chemical(scene, sequence, "tube_ultimate", "mekanism:ultimate_pressurized_tube");
            heat(scene, sequence, "conductor_basic", "mekanism:basic_thermodynamic_conductor");
            heat(scene, sequence, "conductor_ultimate", "mekanism:ultimate_thermodynamic_conductor");
            transporter(scene, sequence);
            plain(scene, sequence);
            mixed(scene, sequence);
            nonetwork(scene, sequence);
            others(scene, sequence);
            real.thenWaitUntil(() -> {
                if (scene.tick()) {
                    helper.fail("Trace not finished yet");
                }
            });
            real.thenExecuteAfter(1, scene::write);
            real.thenSucceed();
        });
    }

    /// Uma linha de `count` transmissores de `id` de A em diante (z = 1), cada cena começa limpa.
    private static void line(Scene scene, String id, int count) {
        scene.clear();
        BlockPos[] cells = {A, B, C, END};
        for (int i = 0; i < count; i++) {
            scene.put(id, cells[i]);
        }
    }

    private static void faces(Scene scene, String name, BlockPos at) {
        for (Direction face : SIDES) {
            for (boolean sneak : new boolean[]{false, true}) {
                scene.click(name + "/face", at, face, sneak, null);
            }
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Energia
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final long[] FLOWS = {0L, 1L, 5L, 1000L, 75_000L, 2_500_000L, 3_000_000_000L, Long.MAX_VALUE};

    private static void energy(Scene scene, java.util.function.BiConsumer<Integer, Runnable> sequence, String name, String id) {
        sequence.accept(1, () -> {
            line(scene, id, 3);
            scene.chamber(END);
            scene.chamber(SIDE);
        });
        sequence.accept(SETTLE, () -> {
            faces(scene, name + "/middle", B);
            scene.click(name + "/end", A, Direction.UP, false, null);
            for (Direction face : SIDES) for (boolean sneak : new boolean[]{false, true}) {
                scene.click(name + "/chamber_end", END, face, sneak, null);
                scene.click(name + "/chamber_side", SIDE, face, sneak, null);
            }
            EnergyNetwork network = (EnergyNetwork) scene.network(B);
            long capacity = network.getCapacity();
            long[] stored = {0L, 1L, 999L, 1000L, capacity / 3, capacity / 2 + 7, capacity - 1, capacity};
            for (long value : stored) {
                for (long flow : FLOWS) {
                    if (value != stored[0] && value != stored[3] && value != capacity / 3 && value != capacity && flow != 0L && flow != 75_000L && flow != Long.MAX_VALUE) {
                        continue;
                    }
                    long flowNow = flow;
                    scene.click(name + "/values", B, Direction.UP, false, () -> {
                        EnergyNetwork current = (EnergyNetwork) scene.network(B);
                        current.energyContainer.setEnergy(value, null);
                        set(current, "prevTransferAmount", flowNow);
                        JsonObject extra = new JsonObject();
                        extra.addProperty("stored", String.valueOf(value));
                        extra.addProperty("prev_transfer", String.valueOf(flowNow));
                        return extra;
                    });
                }
            }
        });
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Fluido
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static void fluid(Scene scene, java.util.function.BiConsumer<Integer, Runnable> sequence, String name, String id) {
        sequence.accept(1, () -> {
            line(scene, id, 3);
            scene.put("mekanismgenerators:heat_generator", END);
        });
        sequence.accept(SETTLE, () -> {
            faces(scene, name + "/middle", B);
            for (Direction face : SIDES) for (boolean sneak : new boolean[]{false, true}) {
                scene.click(name + "/generator", END, face, sneak, null);
            }
            FluidNetwork network = (FluidNetwork) scene.network(B);
            long capacity = network.getCapacity();
            Object[][] contents = {{"", 0L}, {"water", 1L}, {"water", 1000L}, {"water", capacity / 2}, {"lava", capacity / 3}, {"lava", capacity - 1}, {"lava", capacity}, {"water", 16000L}};
            for (Object[] content : contents) {
                String fluid = (String) content[0];
                long amount = (Long) content[1];
                scene.click(name + "/values", B, Direction.UP, false, () -> {
                    FluidNetwork current = (FluidNetwork) scene.network(B);
                    current.getContainer().setContents(fluid.isEmpty() ? FluidResource.EMPTY : FluidResource.of(fluid.equals("water") ? Fluids.WATER : Fluids.LAVA), amount, null);
                    set(current, "prevTransferAmount", amount / 7);
                    JsonObject extra = new JsonObject();
                    extra.addProperty("resource", fluid);
                    extra.addProperty("stored", String.valueOf(amount));
                    extra.addProperty("prev_transfer", String.valueOf(amount / 7));
                    return extra;
                });
            }
        });
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Químico
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static void chemical(Scene scene, java.util.function.BiConsumer<Integer, Runnable> sequence, String name, String id) {
        sequence.accept(1, () -> {
            line(scene, id, 3);
            scene.chamber(END);
        });
        sequence.accept(SETTLE, () -> {
            faces(scene, name + "/middle", B);
            ChemicalNetwork network = (ChemicalNetwork) scene.network(B);
            long capacity = network.getCapacity();
            Object[][] contents = {{"", 0L}, {"mekanism:hydrogen", 1L}, {"mekanism:hydrogen", 1000L}, {"mekanism:oxygen", capacity / 2}, {"mekanism:oxygen", capacity - 1}, {"mekanism:hydrogen", capacity}};
            for (Object[] content : contents) {
                String chemical = (String) content[0];
                long amount = (Long) content[1];
                scene.click(name + "/values", B, Direction.UP, false, () -> {
                    ChemicalNetwork current = (ChemicalNetwork) scene.network(B);
                    current.getContainer().setContents(chemical.isEmpty() ? ChemicalResource.EMPTY : chemical(chemical), amount, null);
                    set(current, "prevTransferAmount", amount / 7);
                    JsonObject extra = new JsonObject();
                    extra.addProperty("resource", chemical);
                    extra.addProperty("stored", String.valueOf(amount));
                    extra.addProperty("prev_transfer", String.valueOf(amount / 7));
                    return extra;
                });
            }
        });
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Calor
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final double[][] HEAT_VALUES = {
          {300.0, 0.0, 0.0},
          {300.5, 12.345, 6.78},
          {1234.5678, 100.0, 0.0},
          {50.0, 0.0, 25.5},
          {100000.0, 3.3333, 6.6667},
          {299.99, 0.001, 0.002},
          {1.0E7, 1.0E6, 1.0E5},
          {0.0, 1.0, 1.0}
    };

    private static void heat(Scene scene, java.util.function.BiConsumer<Integer, Runnable> sequence, String name, String id) {
        sequence.accept(1, () -> {
            line(scene, id, 3);
            scene.put("mekanismgenerators:heat_generator", new BlockPos(3, 2, 1));
        });
        sequence.accept(SETTLE, () -> {
            faces(scene, name + "/middle", B);
            for (Direction face : SIDES) for (boolean sneak : new boolean[]{false, true}) {
                scene.click(name + "/generator", new BlockPos(3, 2, 1), face, sneak, null);
            }
            for (double[] values : HEAT_VALUES) {
                scene.click(name + "/values", B, Direction.UP, false, () -> {
                    DynamicNetwork<?, ?, ?> current = scene.network(B);
                    set(current, "meanTemp", values[0]);
                    set(current, "heatTransferred", values[1]);
                    set(current, "heatLost", values[2]);
                    JsonObject extra = new JsonObject();
                    extra.addProperty("mean_temp", String.valueOf(values[0]));
                    extra.addProperty("heat_transferred", String.valueOf(values[1]));
                    extra.addProperty("heat_lost", String.valueOf(values[2]));
                    return extra;
                });
            }
        });
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Item
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static void transporter(Scene scene, java.util.function.BiConsumer<Integer, Runnable> sequence) {
        String id = "mekanism:basic_logistical_transporter";
        sequence.accept(1, () -> {
            line(scene, id, 3);
            scene.helper.setBlock(END, Blocks.CHEST.defaultBlockState());
            scene.chamber(SIDE);
        });
        sequence.accept(SETTLE, () -> {
            faces(scene, "transporter/middle", B);
            for (Direction face : SIDES) for (boolean sneak : new boolean[]{false, true}) {
                scene.click("transporter/chest", END, face, sneak, null);
                scene.click("transporter/chamber", SIDE, face, sneak, null);
            }
        });
    }

    /// As mesmas linhas sem máquina nenhuma por perto (um baú só no transportador): é o que o port monta em jogo sem configurar lado de máquina, e o chat tem de sair igual.
    private static void plain(Scene scene, java.util.function.BiConsumer<Integer, Runnable> sequence) {
        for (String[] family : new String[][]{{"cable_plain", "mekanism:basic_universal_cable"}, {"pipe_plain", "mekanism:basic_mechanical_pipe"}, {"tube_plain", "mekanism:basic_pressurized_tube"},
                                               {"conductor_plain", "mekanism:basic_thermodynamic_conductor"}, {"transporter_plain", "mekanism:basic_logistical_transporter"}}) {
            sequence.accept(1, () -> {
                line(scene, family[1], 3);
                if (family[0].startsWith("transporter")) {
                    scene.helper.setBlock(END, Blocks.CHEST.defaultBlockState());
                }
            });
            sequence.accept(SETTLE, () -> {
                for (Direction face : new Direction[]{Direction.UP, Direction.NORTH}) {
                    scene.click(family[0] + "/middle", B, face, false, null);
                    scene.click(family[0] + "/end", A, face, false, null);
                }
            });
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Vários vizinhos
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static void mixed(Scene scene, java.util.function.BiConsumer<Integer, Runnable> sequence) {
        // Uma Enrichment Chamber em (3,1,3) tocada por um cabo (norte), um transportador (oeste) e um segundo cabo (leste): duas redes de energia (dois cabos soltos) e uma de itens.
        BlockPos chamber = new BlockPos(3, 1, 3);
        sequence.accept(1, () -> {
            scene.clear();
            scene.chamber(chamber);
            scene.put("mekanism:basic_universal_cable", chamber.north());
            scene.put("mekanism:basic_universal_cable", chamber.north().north());
            scene.put("mekanism:basic_logistical_transporter", chamber.west());
            scene.put("mekanism:advanced_universal_cable", chamber.east());
            // Um baú tocado por dois transportadores (leste e oeste) e por um cabo (sul, que não o liga).
            scene.helper.setBlock(new BlockPos(6, 1, 6), Blocks.CHEST.defaultBlockState());
            scene.put("mekanism:basic_logistical_transporter", new BlockPos(5, 1, 6));
            scene.put("mekanism:basic_logistical_transporter", new BlockPos(7, 1, 6));
            scene.put("mekanism:basic_universal_cable", new BlockPos(6, 1, 7));
        });
        sequence.accept(SETTLE, () -> {
            for (Direction face : SIDES) for (boolean sneak : new boolean[]{false, true}) {
                scene.click("mixed/chamber", chamber, face, sneak, null);
                scene.click("mixed/chest", new BlockPos(6, 1, 6), face, sneak, null);
            }
            scene.click("mixed/chamber", chamber, Direction.UP, true, null);
        });
    }

    /// Um transmissor recém-posto, no mesmo tick do clique: ainda sem rede.
    private static void nonetwork(Scene scene, java.util.function.BiConsumer<Integer, Runnable> sequence) {
        sequence.accept(1, () -> scene.clear());
        for (String id : new String[]{"mekanism:basic_universal_cable", "mekanism:basic_mechanical_pipe", "mekanism:basic_pressurized_tube", "mekanism:basic_thermodynamic_conductor",
                                      "mekanism:basic_logistical_transporter"}) {
            sequence.accept(2, () -> scene.click("nonetwork/" + id.substring(id.indexOf(':') + 1), B, Direction.UP, false, () -> {
                scene.put(id, B);
                return null;
            }));
            sequence.accept(2, () -> scene.helper.setBlock(B, Blocks.AIR.defaultBlockState()));
        }
    }

    private static void others(Scene scene, java.util.function.BiConsumer<Integer, Runnable> sequence) {
        sequence.accept(1, () -> {
            scene.clear();
            scene.helper.setBlock(A, Blocks.STONE.defaultBlockState());
            scene.helper.setBlock(B, Blocks.CHEST.defaultBlockState());
        });
        sequence.accept(SETTLE, () -> {
            for (Direction face : SIDES) {
                for (boolean sneak : new boolean[]{false, true}) {
                    scene.click("others/stone", A, face, sneak, null);
                    scene.click("others/chest", B, face, sneak, null);
                    scene.click("others/air", C, face, sneak, null);
                }
            }
        });
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // O modo de depuração (`MekanismAPI.debug`, ligado pelo `/mek debug`): o ramo do leitor agachado, os logs do registro e o comando
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final String[][] DEBUG_ROWS = {{"mekanism:basic_universal_cable", "0"}, {"mekanism:basic_mechanical_pipe", "2"}, {"mekanism:basic_pressurized_tube", "4"},
                                                  {"mekanism:basic_thermodynamic_conductor", "6"}, {"mekanism:basic_logistical_transporter", "8"}};

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH + "_debug")
    @TestHolder(description = "Records MekanismAPI.debug: the sneaking Network Reader branch, the registry debug logs and the /mek debug command.")
    public static void networkReaderDebug(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> {
            Scene scene = new Scene(helper);
            scene.fileName = "network_reader_debug.jsonl";
            scene.meta = "{\"meta\":\"commit " + COMMIT + ". MekanismAPI.debug. case: the step. registry: TransmitterNetworkRegistry.networks in iterator order (class, uuid, uuid hashCode, transmitters, acceptors, getTextComponent as "
                  + "JSON). components: TransmitterNetworkRegistry.toComponents() as JSON. log: the Mekanism.logger lines since the previous step. chat: the system messages the player received. For the clicks: face, sneak, "
                  + "result (InteractionResult of useItemOn). For the commands: command, level (permission level of the source), mek_usable and debug_usable (CommandNode.canUse), debug_before and debug_after.\"}";
            scene.captureLogs();
            var real = helper.startSequence();
            java.util.function.BiConsumer<Integer, Runnable> sequence = (ticks, action) -> scene.tasks.add(new Scene.Task(ticks, action));
            BlockPos stone = new BlockPos(4, 0, 4);
            // 1. Sem depuração: dez redes, uma por vez (duas linhas por família, separadas por um bloco vazio), cada uma com a nota do registro depois: o `.networks` nasce na ordem conhecida, e o
            // teste do port repete as entradas e as saídas na mesma ordem. Sem log nenhum.
            sequence.accept(1, () -> {
                MekanismAPI.debug = false;
                scene.clear();
            });
            for (String[] row : DEBUG_ROWS) {
                int z = Integer.parseInt(row[1]);
                for (int[] segment : new int[][]{{1, 2}, {4, 5, 6}}) {
                    sequence.accept(1, () -> {
                        for (int x : segment) {
                            scene.put(row[0], new BlockPos(x, 1, z));
                        }
                    });
                    sequence.accept(4, () -> scene.note("build/" + row[0].substring(row[0].indexOf(':') + 1) + "/" + segment[0], false));
                }
            }
            sequence.accept(SETTLE, () -> scene.note("built/debug_off", false));
            sequence.accept(1, () -> scene.click("stone/sneak/debug_off", stone, Direction.UP, true, null));
            // 2. Com depuração: o clique agachado em qualquer bloco sem block entity, e o que não é esse ramo.
            sequence.accept(1, () -> MekanismAPI.debug = true);
            sequence.accept(2, () -> scene.click("stone/sneak/debug_on", stone, Direction.UP, true, null));
            sequence.accept(2, () -> scene.click("stone/sneak/debug_on_side", stone, Direction.NORTH, true, null));
            sequence.accept(2, () -> scene.click("stone/nosneak/debug_on", stone, Direction.UP, false, null));
            sequence.accept(2, () -> scene.click("air/sneak/debug_on", new BlockPos(3, 1, 3), Direction.UP, true, null));
            sequence.accept(2, () -> scene.click("cable/sneak/debug_on", new BlockPos(2, 1, 0), Direction.UP, true, null));
            sequence.accept(2, () -> scene.helper.setBlock(new BlockPos(7, 1, 3), Blocks.CHEST.defaultBlockState()));
            sequence.accept(2, () -> scene.click("chest/sneak/debug_on", new BlockPos(7, 1, 3), Direction.UP, true, null));
            // 3. O que o registro escreve no log com a depuração ligada: um cabo que une duas redes, um tubo solto, um cabo que entra numa rede, um cabo que sai.
            sequence.accept(2, () -> scene.logs());
            sequence.accept(1, () -> scene.put("mekanism:basic_universal_cable", new BlockPos(3, 1, 0)));
            sequence.accept(SETTLE, () -> scene.note("merge_two_networks", true));
            sequence.accept(1, () -> scene.put("mekanism:basic_pressurized_tube", new BlockPos(8, 1, 4)));
            sequence.accept(SETTLE, () -> scene.note("lone_tube", true));
            sequence.accept(1, () -> scene.put("mekanism:basic_mechanical_pipe", new BlockPos(7, 1, 2)));
            sequence.accept(SETTLE, () -> scene.note("pipe_joins_one_network", true));
            sequence.accept(1, () -> scene.helper.setBlock(new BlockPos(4, 1, 6), Blocks.AIR.defaultBlockState()));
            sequence.accept(SETTLE, () -> scene.note("conductor_removed", true));
            sequence.accept(1, () -> scene.click("stone/sneak/after_changes", stone, Direction.UP, true, null));
            // 4. O comando: um jogador sem permissão, um com o nível de operador, duas vezes (liga e desliga), e o nível de moderador.
            sequence.accept(2, () -> MekanismAPI.debug = false);
            sequence.accept(1, () -> scene.command("command/level_all", "mek debug", PermissionLevel.ALL));
            sequence.accept(2, () -> scene.command("command/level_moderators", "mek debug", PermissionLevel.MODERATORS));
            sequence.accept(2, () -> scene.command("command/level_gamemasters_on", "mek debug", PermissionLevel.GAMEMASTERS));
            sequence.accept(2, () -> scene.command("command/level_gamemasters_off", "mek debug", PermissionLevel.GAMEMASTERS));
            sequence.accept(2, () -> scene.command("command/level_owners_on", "mek debug", PermissionLevel.OWNERS));
            sequence.accept(2, () -> scene.command("command/mek_alone", "mek", PermissionLevel.OWNERS));
            sequence.accept(2, () -> scene.command("command/unknown_sub", "mek nothing", PermissionLevel.OWNERS));
            sequence.accept(2, () -> scene.command("command/extra_arg", "mek debug extra", PermissionLevel.OWNERS));
            sequence.accept(2, () -> MekanismAPI.debug = false);
            real.thenWaitUntil(() -> {
                if (scene.tick()) {
                    helper.fail("Trace not finished yet");
                }
            });
            real.thenExecuteAfter(1, scene::write);
            real.thenSucceed();
        });
    }
}
