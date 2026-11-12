package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.api.text.EnumColor;
import mekanism.common.block.transmitter.BlockTransmitter;
import mekanism.common.content.network.transmitter.BufferedTransmitter;
import mekanism.common.content.network.transmitter.Transmitter;
import mekanism.common.lib.transmitter.ConnectionType;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.lib.transmitter.acceptor.AcceptorCache;
import mekanism.common.registries.MekanismItems;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.transmitter.TileEntityTransmitter;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.ByteTag;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.IntArrayTag;
import net.minecraft.nbt.IntTag;
import net.minecraft.nbt.StringTag;
import net.minecraft.nbt.Tag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientboundSystemChatPacket;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.capabilities.BlockCapability;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.GameTestPlayer;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Frente `rede-conexao`: grava a conexao por lado do `Transmitter` (os bytes `currentTransmitterConnections` e `currentAcceptorConnections`, os `ConnectionType` crus e os visiveis, o redstone reativo,
/// o ciclo do Configurator e o NBT) para o cabo universal e o cano mecanico, com vizinhos de varios tipos. Um `runGameTestServer`, os arquivos `transmitter_*.jsonl`.
///
/// Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` so na hora de rodar o `runGameTestServer` e apague-o depois (`oficial_gametest_rastro.sh`).
@ForEachTest(groups = "trace")
public class TransmitterConnectionTraceTest {

    private static final int TIMEOUT = 900 * SharedConstants.TICKS_PER_SECOND;
    private static final String BATCH = "trace_transmitter_connection";
    private static final BlockPos T = new BlockPos(2, 2, 2);
    private static final int SETTLE = 6;
    private static final String COMMIT = "b2d04e166";
    private static final Direction[] SIDES = Direction.values();

    private static final String CABLE = "mekanism:basic_universal_cable";
    private static final String CABLE_ADV = "mekanism:advanced_universal_cable";
    private static final String PIPE = "mekanism:basic_mechanical_pipe";
    private static final String PIPE_ADV = "mekanism:advanced_mechanical_pipe";
    private static final String MACHINE = "mekanism:enrichment_chamber";
    private static final String CUBE = "mekanism:basic_energy_cube";
    private static final String TANK = "mekanism:basic_fluid_tank";

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(5, 5, 5).fill(0, 0, 0, 4, 0, 4, Blocks.STONE.defaultBlockState()));
    }

    private static void write(String name, List<String> lines) {
        try {
            Path file = Path.of("trace", name + ".jsonl");
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static Item item(String id) {
        return BuiltInRegistries.ITEM.getValue(Identifier.parse(id));
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

    private static boolean flag(Object target, String name) {
        try {
            return field(target.getClass(), name).getBoolean(target);
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }

    private static JsonElement component(Component component) {
        return scrubUuid(ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, component).getOrThrow());
    }

    private static JsonElement scrubUuid(JsonElement element) {
        if (element.isJsonObject()) {
            JsonObject in = element.getAsJsonObject();
            for (String key : new ArrayList<>(in.keySet())) {
                if (key.equals("uuid")) {
                    JsonArray zero = new JsonArray();
                    for (int i = 0; i < 4; i++) {
                        zero.add(0);
                    }
                    in.add(key, zero);
                } else {
                    scrubUuid(in.get(key));
                }
            }
        } else if (element.isJsonArray()) {
            for (JsonElement child : element.getAsJsonArray()) {
                scrubUuid(child);
            }
        }
        return element;
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static JsonArray drainOverlay(GameTestPlayer player) {
        JsonArray overlay = new JsonArray();
        for (Packet<?> packet : (List<Packet<?>>) (List) ((java.util.stream.Stream) player.getOutboundPackets((Class) Packet.class)).toList()) {
            if (packet instanceof ClientboundSystemChatPacket system && system.overlay()) {
                overlay.add(component(system.content()));
            }
        }
        player.clearOutboundPackets();
        return overlay;
    }

    private static JsonObject result(InteractionResult result) {
        JsonObject out = new JsonObject();
        out.addProperty("class", result.getClass().getSimpleName());
        out.addProperty("consumes_action", result.consumesAction());
        return out;
    }

    private static JsonArray names(Iterable<String> values) {
        JsonArray array = new JsonArray();
        for (String value : values) {
            array.add(value);
        }
        return array;
    }

    private static JsonObject tagJson(CompoundTag tag) {
        JsonObject out = new JsonObject();
        List<String> keys = new ArrayList<>(tag.keySet());
        java.util.Collections.sort(keys);
        for (String key : keys) {
            Tag value = tag.get(key);
            JsonObject entry = new JsonObject();
            if (value instanceof ByteTag b) {
                entry.addProperty("byte", b.value());
            } else if (value instanceof IntTag i) {
                entry.addProperty("int", i.value());
            } else if (value instanceof StringTag s) {
                entry.addProperty("string", s.value());
            } else if (value instanceof IntArrayTag a) {
                JsonArray array = new JsonArray();
                for (int v : a.getAsIntArray()) {
                    array.add(v);
                }
                entry.add("int_array", array);
            } else {
                entry.addProperty("other", value.toString());
            }
            out.add(key, entry);
        }
        return out;
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Cena
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final class Scene {

        final List<String> lines = new ArrayList<>();
        final List<String> placeLog = new ArrayList<>();
        MekGameTestHelper helper;
        GameTestPlayer placer;
        /// O jogador da cena anterior, que sai da frente para nao barrar a colocacao do transmissor.
        GameTestPlayer clicker;

        void reset(MekGameTestHelper helper) {
            this.helper = helper;
            for (int x = 0; x <= 4; x++) {
                for (int y = 1; y <= 4; y++) {
                    for (int z = 0; z <= 4; z++) {
                        helper.setBlock(new BlockPos(x, y, z), Blocks.AIR.defaultBlockState());
                    }
                }
            }
            for (ItemEntity entity : helper.getLevel().getEntitiesOfClass(ItemEntity.class, new AABB(helper.absolutePos(T)).inflate(8))) {
                entity.discard();
            }
            if (clicker != null) {
                Vec3 away = Vec3.atBottomCenterOf(abs(new BlockPos(2, 1, 12)));
                clicker.snapTo(away.x, away.y, away.z, 0, 0);
            }
            if (placer == null) {
                placer = (GameTestPlayer) helper.makeMockPlayerLookingAt(T.below(), Direction.UP);
                placer.gameMode.changeGameModeForPlayer(GameType.CREATIVE);
            }
        }

        ServerLevel level() {
            return helper.getLevel();
        }

        BlockPos abs(BlockPos rel) {
            return helper.absolutePos(rel);
        }

        TileEntityTransmitter tile(BlockPos rel) {
            return level().getBlockEntity(abs(rel)) instanceof TileEntityTransmitter tile ? tile : null;
        }

        /// Poe um bloco como um jogador o poria (o item clicado no ar da celula, que o `BlockPlaceContext` troca pela propria celula) ou, para o vanilla, com `setBlock`. Devolve o nome do `facing` do tile.
        String place(String id, BlockPos rel, Direction facing) {
            switch (id) {
                case "air":
                    return "-";
                case "stone":
                    helper.setBlock(rel, Blocks.STONE.defaultBlockState());
                    return "-";
                case "chest":
                    helper.setBlock(rel, Blocks.CHEST.defaultBlockState());
                    return "-";
                case "redstone":
                    helper.setBlock(rel, Blocks.REDSTONE_BLOCK.defaultBlockState());
                    return "-";
                default:
                    break;
            }
            Vec3 away = Vec3.atBottomCenterOf(abs(new BlockPos(2, 1, -6)));
            placer.snapTo(away.x, away.y, away.z, facing.getOpposite().toYRot(), 0);
            placer.setShiftKeyDown(false);
            ItemStack stack = new ItemStack(item(id));
            placer.setItemInHand(InteractionHand.MAIN_HAND, stack);
            BlockPos at = abs(rel);
            BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(at).relative(Direction.UP, 0.5), Direction.UP, at, false);
            InteractionResult used = stack.useOn(new net.minecraft.world.item.context.UseOnContext(level(), placer, InteractionHand.MAIN_HAND, stack, hit));
            if (level().getBlockState(at).isAir()) {
                placeLog.add(id + "@" + rel.toShortString() + ":" + used.getClass().getSimpleName() + ":" + stack.getCount() + ":air");
                Block block = BuiltInRegistries.BLOCK.getValue(Identifier.parse(id));
                helper.setBlock(rel, block.defaultBlockState());
            }
            BlockEntity be = level().getBlockEntity(abs(rel));
            if (be instanceof TileEntityMekanism mek && mek.isDirectional()) {
                return mek.getDirection().getSerializedName();
            }
            return level().getBlockState(abs(rel)).isAir() ? "missing" : "-";
        }

        /// O estado de um transmissor: os bytes, os tipos crus e os visiveis, o redstone e as capabilities por lado.
        JsonObject snap(BlockPos rel) {
            TileEntityTransmitter tile = tile(rel);
            JsonObject o = new JsonObject();
            if (tile == null) {
                o.addProperty("present", false);
                return o;
            }
            Transmitter<?, ?, ?> t = tile.getTransmitter();
            o.addProperty("present", true);
            o.addProperty("block", BuiltInRegistries.BLOCK.getKey(tile.getBlockState().getBlock()).getPath());
            o.addProperty("cur_t", t.currentTransmitterConnections & 0xFF);
            o.addProperty("cur_a", t.getAcceptorCache().currentAcceptorConnections & 0xFF);
            o.addProperty("all", t.getAllCurrentConnections() & 0xFF);
            JsonArray raw = new JsonArray();
            JsonArray types = new JsonArray();
            JsonArray canConnect = new JsonArray();
            JsonArray toAcceptor = new JsonArray();
            JsonArray extract = new JsonArray();
            JsonArray insert = new JsonArray();
            JsonArray caps = new JsonArray();
            for (Direction side : SIDES) {
                raw.add(t.getConnectionTypeRaw(side).getSerializedName());
                types.add(t.getConnectionType(side).getSerializedName());
                canConnect.add(t.canConnect(side));
                toAcceptor.add(t.canConnectToAcceptor(side));
                extract.add(tile.canExtract(side));
                insert.add(tile.canInsert(side));
                caps.add(t.getAcceptorCache().getConnectedAcceptor(side) != null);
            }
            o.add("raw", raw);
            o.add("types", types);
            o.add("can_connect", canConnect);
            o.add("can_connect_to_acceptor", toAcceptor);
            o.add("can_extract", extract);
            o.add("can_insert", insert);
            o.addProperty("can_extract_null", tile.canExtract(null));
            o.addProperty("can_insert_null", tile.canInsert(null));
            o.add("caps", caps);
            o.addProperty("has_pull", flag(t, "hasPullSide"));
            o.addProperty("reactive", flag(t, "redstoneReactive"));
            o.addProperty("powered", flag(t, "redstonePowered"));
            o.addProperty("redstone_set", flag(t, "redstoneSet"));
            o.addProperty("activated", t.isRedstoneActivated());
            o.addProperty("has_network", t.hasTransmitterNetwork());
            return o;
        }

        JsonObject nbt(BlockPos rel) {
            TileEntityTransmitter tile = tile(rel);
            if (tile == null) {
                return new JsonObject();
            }
            TagValueOutput out = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, level().registryAccess());
            tile.getTransmitter().write(out);
            JsonObject o = new JsonObject();
            o.add("write", tagJson(out.buildResult()));
            TagValueOutput reduced = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, level().registryAccess());
            tile.getTransmitter().writeReducedUpdatedTag(reduced);
            JsonObject reducedJson = tagJson(reduced.buildResult());
            reducedJson.remove("network");
            o.add("reduced", reducedJson);
            return o;
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Fatos estaticos
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records ConnectionType, TransmissionType and the facts of every transmitter block.")
    public static void types(final DynamicTest test) {
        template(test);
        Scene scene = new Scene();
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  scene.reset(helper);
                  scene.lines.add("{\"meta\":\"commit " + COMMIT + ". kind=connection_type: ordinal, name, serialized, color (EnumColor name), lang key, can_accept, can_send_to, next, previous, next_skipping_none "
                        + "(getNext with a predicate that rejects NONE), adjust_plus_2. kind=by_id: ConnectionType.BY_ID and TransmissionType.BY_ID for the given id. kind=transmission_type: ordinal, name, transmission, "
                        + "serialized, lang key, gui_texture. kind=block: id, supported types, handles_redstone, can_have_incompatible_networks (BufferedTransmitter only), capability name of the AcceptorCache, default raw types and bytes.\"}");
                  for (ConnectionType type : ConnectionType.VALUES) {
                      JsonObject o = new JsonObject();
                      o.addProperty("kind", "connection_type");
                      o.addProperty("ordinal", type.ordinal());
                      o.addProperty("name", type.name());
                      o.addProperty("serialized", type.getSerializedName());
                      o.addProperty("lang_key", type.getTranslationKey());
                      o.addProperty("color", colorOf(type));
                      o.addProperty("can_accept", type.canAccept());
                      o.addProperty("can_send_to", type.canSendTo());
                      o.addProperty("next", type.getNext().name());
                      o.addProperty("previous", type.getPrevious().name());
                      o.addProperty("next_skipping_none", type.getNext(t -> t != ConnectionType.NONE).name());
                      o.addProperty("previous_skipping_none", type.getPrevious(t -> t != ConnectionType.NONE).name());
                      o.addProperty("adjust_plus_2", type.adjust(2).name());
                      o.addProperty("adjust_minus_5", type.adjust(-5).name());
                      o.add("text", component(type.getTextComponent()));
                      scene.lines.add(o.toString());
                  }
                  for (int id = -9; id <= 12; id++) {
                      JsonObject o = new JsonObject();
                      o.addProperty("kind", "by_id");
                      o.addProperty("id", id);
                      o.addProperty("connection", ConnectionType.BY_ID.apply(id).name());
                      o.addProperty("transmission", TransmissionType.BY_ID.apply(id).name());
                      scene.lines.add(o.toString());
                  }
                  for (TransmissionType type : TransmissionType.VALUES) {
                      JsonObject o = new JsonObject();
                      o.addProperty("kind", "transmission_type");
                      o.addProperty("ordinal", type.ordinal());
                      o.addProperty("name", type.name());
                      o.addProperty("network_name", type.getName());
                      o.addProperty("transmission", type.getTransmission());
                      o.addProperty("serialized", type.getSerializedName());
                      o.addProperty("lang_key", type.getTranslationKey());
                      o.addProperty("gui_texture", type.guiTexture().toString());
                      scene.lines.add(o.toString());
                  }
                  for (Block block : BuiltInRegistries.BLOCK) {
                      if (!(block instanceof BlockTransmitter<?>) || !BuiltInRegistries.BLOCK.getKey(block).getNamespace().equals("mekanism")) {
                          continue;
                      }
                      scene.helper.setBlock(T, block.defaultBlockState());
                      TileEntityTransmitter tile = scene.tile(T);
                      Transmitter<?, ?, ?> t = tile.getTransmitter();
                      JsonObject o = new JsonObject();
                      o.addProperty("kind", "block");
                      o.addProperty("id", BuiltInRegistries.BLOCK.getKey(block).getPath());
                      List<String> supported = new ArrayList<>();
                      for (TransmissionType type : TransmissionType.VALUES) {
                          if (t.getSupportedTransmissionTypes().contains(type)) {
                              supported.add(type.name());
                          }
                      }
                      o.add("supported", names(supported));
                      o.addProperty("handles_redstone", t.handlesRedstone());
                      if (t instanceof BufferedTransmitter<?, ?, ?, ?> buffered) {
                          try {
                              Method method = BufferedTransmitter.class.getDeclaredMethod("canHaveIncompatibleNetworks");
                              method.setAccessible(true);
                              o.addProperty("can_have_incompatible_networks", (Boolean) method.invoke(buffered));
                          } catch (ReflectiveOperationException e) {
                              throw new IllegalStateException(e);
                          }
                      } else {
                          o.add("can_have_incompatible_networks", JsonNull.INSTANCE);
                      }
                      try {
                          BlockCapability<?, ?> capability = (BlockCapability<?, ?>) field(AcceptorCache.class, "capability").get(t.getAcceptorCache());
                          o.addProperty("capability", capability.name().toString());
                      } catch (IllegalAccessException e) {
                          throw new IllegalStateException(e);
                      }
                      JsonObject state = scene.snap(T);
                      o.add("raw", state.get("raw"));
                      o.add("types", state.get("types"));
                      o.addProperty("cur_t", state.get("cur_t").getAsInt());
                      o.addProperty("cur_a", state.get("cur_a").getAsInt());
                      scene.lines.add(o.toString());
                  }
              })
              .thenExecuteAfter(1, () -> write("transmitter_types", scene.lines))
              .thenSucceed());
    }

    private static String colorOf(ConnectionType type) {
        try {
            Field color = field(ConnectionType.class, "color");
            return ((EnumColor) color.get(type)).name();
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Vizinhos
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private record Placement(Direction side, String kind, Direction facing) {
    }

    private static final Direction[] FACINGS = {Direction.NORTH, Direction.EAST, Direction.SOUTH, Direction.WEST};

    private static List<String[]> kindsFor(String transmitter) {
        List<String[]> kinds = new ArrayList<>();
        boolean cable = transmitter.equals(CABLE);
        for (String kind : cable ? new String[]{CABLE, CABLE_ADV, PIPE} : new String[]{PIPE, PIPE_ADV, CABLE}) {
            kinds.add(new String[]{kind, "north"});
        }
        kinds.add(new String[]{cable ? CUBE : TANK, "north"});
        for (Direction facing : FACINGS) {
            kinds.add(new String[]{MACHINE, facing.getSerializedName()});
        }
        if (!cable) {
            kinds.add(new String[]{CUBE, "north"});
        }
        kinds.add(new String[]{"stone", "north"});
        kinds.add(new String[]{"chest", "north"});
        kinds.add(new String[]{"air", "north"});
        return kinds;
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the connections of a cable and a pipe with one neighbor on each side, placed before and after the transmitter.")
    public static void neighbors(final DynamicTest test) {
        template(test);
        Scene scene = new Scene();
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecuteAfter(1, () -> scene.lines.add("{\"meta\":\"commit " + COMMIT + ". One case per line. case: transmitter/neighbor kind/facing/side/order. order=neighbor_first places the neighbor then the transmitter (T.onAdded "
                  + "refreshes), order=transmitter_first places the transmitter, waits 2 ticks, then the neighbor (neighborChanged). Machines are placed by a player at the given facing (facing_real is the tile's). "
                  + "Single neighbor cases carry placements=[{side, kind, facing}]; combos carry several. After 6 ticks: t (the transmitter state: cur_t, cur_a, all, raw, types, can_connect, can_connect_to_acceptor, "
                  + "can_extract, can_insert, caps (the capability found at each side by the AcceptorCache, the world input of the port), has_pull, reactive, powered, redstone_set, activated), n (each neighbor "
                  + "that is a transmitter, by side). Sides are in Direction order: down, up, north, south, west, east.\"}"));
            for (String transmitter : List.of(CABLE, PIPE)) {
                for (String[] kind : kindsFor(transmitter)) {
                    for (Direction side : SIDES) {
                        for (boolean neighborFirst : new boolean[]{true, false}) {
                            if (kind[0].equals("air") && !neighborFirst) {
                                continue;
                            }
                            Placement placement = new Placement(side, kind[0], Direction.byName(kind[1]));
                            String name = transmitter.substring(transmitter.indexOf(':') + 1) + "/" + kind[0].substring(kind[0].indexOf(':') + 1) + "/" + kind[1] + "/" + side.getSerializedName() + "/"
                                  + (neighborFirst ? "neighbor_first" : "transmitter_first");
                            stepsFor((ticks, step) -> sequence.thenExecuteAfter(ticks, step), scene, helper, name, transmitter, List.of(placement), neighborFirst);
                        }
                    }
                }
                // Combinacoes
                List<List<Placement>> combos = List.of(
                      List.of(new Placement(Direction.EAST, transmitter, Direction.NORTH), new Placement(Direction.WEST, transmitter, Direction.NORTH)),
                      List.of(new Placement(Direction.NORTH, transmitter, Direction.NORTH), new Placement(Direction.EAST, transmitter, Direction.NORTH), new Placement(Direction.SOUTH, MACHINE, Direction.NORTH),
                            new Placement(Direction.WEST, "stone", Direction.NORTH), new Placement(Direction.UP, "chest", Direction.NORTH), new Placement(Direction.DOWN, transmitter.equals(CABLE) ? CUBE : TANK, Direction.NORTH)),
                      List.of(new Placement(Direction.UP, transmitter, Direction.NORTH), new Placement(Direction.DOWN, transmitter.equals(CABLE) ? PIPE : CABLE, Direction.NORTH)),
                      List.of(new Placement(Direction.EAST, transmitter.equals(CABLE) ? CABLE_ADV : PIPE_ADV, Direction.NORTH), new Placement(Direction.WEST, transmitter, Direction.NORTH),
                            new Placement(Direction.NORTH, MACHINE, Direction.SOUTH), new Placement(Direction.SOUTH, MACHINE, Direction.WEST)));
                int index = 0;
                for (List<Placement> combo : combos) {
                    for (boolean neighborFirst : new boolean[]{true, false}) {
                        stepsFor((ticks, step) -> sequence.thenExecuteAfter(ticks, step), scene, helper, transmitter.substring(transmitter.indexOf(':') + 1) + "/combo" + index + "/" + (neighborFirst ? "neighbor_first" : "transmitter_first"), transmitter, combo,
                              neighborFirst);
                    }
                    index++;
                }
            }
            sequence.thenExecuteAfter(1, () -> write("transmitter_neighbors", scene.lines));
            sequence.thenSucceed();
        });
    }

    private static void stepsFor(java.util.function.BiConsumer<Integer, Runnable> sequence, Scene scene, MekGameTestHelper helper, String name, String transmitter, List<Placement> placements, boolean neighborFirst) {
        JsonArray facings = new JsonArray();
        sequence.accept(1, () -> {
            scene.reset(helper);
            while (facings.size() > 0) {
                facings.remove(0);
            }
            if (neighborFirst) {
                for (Placement p : placements) {
                    facings.add(scene.place(p.kind, T.relative(p.side), p.facing));
                }
                scene.place(transmitter, T, Direction.NORTH);
            } else {
                scene.place(transmitter, T, Direction.NORTH);
            }
        });
        if (!neighborFirst) {
            sequence.accept(2, () -> {
                for (Placement p : placements) {
                    facings.add(scene.place(p.kind, T.relative(p.side), p.facing));
                }
            });
        }
        sequence.accept(SETTLE, () -> {
            JsonObject line = new JsonObject();
            line.addProperty("case", name);
            JsonArray list = new JsonArray();
            JsonObject neighbors = new JsonObject();
            int i = 0;
            for (Placement p : placements) {
                JsonObject o = new JsonObject();
                o.addProperty("side", p.side.getSerializedName());
                o.addProperty("kind", p.kind);
                o.addProperty("facing", p.facing.getSerializedName());
                o.addProperty("facing_real", i < facings.size() ? facings.get(i).getAsString() : "?");
                list.add(o);
                i++;
                if (scene.tile(T.relative(p.side)) != null) {
                    neighbors.add(p.side.getSerializedName(), scene.snap(T.relative(p.side)));
                }
            }
            line.add("placements", list);
            line.add("t", scene.snap(T));
            line.add("n", neighbors);
            line.add("place_log", names(scene.placeLog));
            scene.placeLog.clear();
            scene.lines.add(line.toString());
        });
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Configurator
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static void lookAt(Scene scene, GameTestPlayer player, Direction side) {
        BlockPos target = scene.abs(T);
        player.snapTo(Vec3.upFromBottomCenterOf(target.relative(side), -player.getEyeHeight()));
        player.lookAt(net.minecraft.commands.arguments.EntityAnchorArgument.Anchor.EYES, Vec3.atCenterOf(target));
    }

    /// O clique no lado `side` (a face do `BlockHitResult`) com o jogador olhando para o transmissor de `aimFrom`, a celula vizinha do lado de onde ele olha.
    private static InteractionResult click(Scene scene, GameTestPlayer player, Direction side, Direction aimFrom, boolean sneak) {
        lookAt(scene, player, aimFrom);
        player.setShiftKeyDown(sneak);
        ItemStack stack = MekanismItems.CONFIGURATOR.asStack();
        player.setItemInHand(InteractionHand.MAIN_HAND, stack);
        BlockPos abs = scene.abs(T);
        BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(abs).relative(side, 0.5), side, abs, false);
        player.clearOutboundPackets();
        return player.gameMode.useItemOn(player, scene.level(), stack, InteractionHand.MAIN_HAND, hit);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the Configurator cycling the connection type of each side of a cable and a pipe.")
    public static void configurator(final DynamicTest test) {
        template(test);
        Scene scene = new Scene();
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecuteAfter(1, () -> scene.lines.add("{\"meta\":\"commit " + COMMIT + ". One click per line through ServerPlayerGameMode.useItemOn with the Configurator (the default mode, configurate items), a player standing "
                  + "in the neighbor cell of the clicked side looking at the transmitter. case: transmitter/setup/side/click number/sneak. setup=alone (no neighbors), setup=arm_east (the busy setup, sneaking only, the player aiming at the transmitter from the east cell while the hit result names the clicked side: the east arm wins when it exists), setup=busy (cable east, machine up facing north, chest north, "
                  + "same type west, and a stone south). result: InteractionResult of useItemOn. overlay: the action bar messages the player received. before and after: the transmitter state right after the click "
                  + "(before is the state before the click), settled: 4 ticks after. changed: the sides whose raw type changed in the click, with from and to.\"}"));
            for (String transmitter : List.of(CABLE, PIPE)) {
                for (String setup : List.of("alone", "busy", "arm_east")) {
                    for (Direction side : SIDES) {
                        for (boolean sneak : new boolean[]{true, false}) {
                            if (setup.equals("arm_east") && !sneak) {
                                continue;
                            }
                            int clicks = setup.equals("arm_east") ? 3 : sneak ? 5 : 2;
                            Direction aimFrom = setup.equals("arm_east") ? Direction.EAST : side;
                            String base = transmitter.substring(transmitter.indexOf(':') + 1) + "/" + setup + "/" + side.getSerializedName();
                            GameTestPlayer[] player = new GameTestPlayer[1];
                            sequence.thenExecuteAfter(1, () -> {
                                scene.reset(helper);
                                if (!setup.equals("alone")) {
                                    scene.place(transmitter.equals(CABLE) ? CABLE : PIPE, T.relative(Direction.EAST), Direction.NORTH);
                                    scene.place(MACHINE, T.relative(Direction.UP), Direction.NORTH);
                                    scene.place("chest", T.relative(Direction.NORTH), Direction.NORTH);
                                    scene.place(transmitter, T.relative(Direction.WEST), Direction.NORTH);
                                    scene.place("stone", T.relative(Direction.SOUTH), Direction.NORTH);
                                }
                                scene.place(transmitter, T, Direction.NORTH);
                                player[0] = (GameTestPlayer) helper.makeMockPlayerLookingAt(T, side.getOpposite());
                                player[0].gameMode.changeGameModeForPlayer(GameType.SURVIVAL);
                                scene.clicker = player[0];
                            });
                            sequence.thenExecuteAfter(SETTLE, () -> {
                            });
                            for (int n = 1; n <= clicks; n++) {
                                int number = n;
                                JsonObject line = new JsonObject();
                                sequence.thenExecuteAfter(1, () -> {
                                    JsonObject before = scene.snap(T);
                                    InteractionResult result = click(scene, player[0], side, aimFrom, sneak);
                                    line.addProperty("case", base + "/" + number + "/" + (sneak ? "sneak" : "no_sneak"));
                                    line.add("result", result(result));
                                    JsonObject after = scene.snap(T);
                                    JsonArray changed = new JsonArray();
                                    for (int i = 0; i < 6 && before.has("raw") && after.has("raw"); i++) {
                                        String from = before.getAsJsonArray("raw").get(i).getAsString();
                                        String to = after.getAsJsonArray("raw").get(i).getAsString();
                                        if (!from.equals(to)) {
                                            JsonObject c = new JsonObject();
                                            c.addProperty("side", SIDES[i].getSerializedName());
                                            c.addProperty("from", from);
                                            c.addProperty("to", to);
                                            changed.add(c);
                                        }
                                    }
                                    line.add("changed", changed);
                                    line.add("before", before);
                                    line.add("after", after);
                                    if (before.has("raw")) {
                                        line.add("nbt", scene.nbt(T));
                                    }
                                    line.add("place_log", names(scene.placeLog));
                                    scene.placeLog.clear();
                                });
                                sequence.thenExecuteAfter(4, () -> {
                                    line.add("overlay", drainOverlay(player[0]));
                                    line.add("settled", scene.snap(T));
                                    scene.lines.add(line.toString());
                                });
                            }
                        }
                    }
                }
            }
            sequence.thenExecuteAfter(1, () -> write("transmitter_configurator", scene.lines));
            sequence.thenSucceed();
        });
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Redstone
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the redstone reactive toggle of a cable and a pipe and a redstone block turned on and off beside them.")
    public static void redstone(final DynamicTest test) {
        template(test);
        Scene scene = new Scene();
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecuteAfter(1, () -> scene.lines.add("{\"meta\":\"commit " + COMMIT + ". One scenario per line, steps recorded in order. setup: cable/pipe with a same type transmitter east, a machine "
                  + "(or a tank for the pipe) west facing north and a chest up. Steps: start; right click (a non sneaking Configurator click on the transmitter, which toggles redstoneReactive and sends the overlay message) "
                  + "only when reactive_first=true; power_on (a redstone block at the given side, 4 ticks settled); power_off (the block removed); toggle (a second right click, powered or not). "
                  + "Each step: the transmitter state ('t'), the east neighbor ('east') and the overlay messages.\"}"));
            for (String transmitter : List.of(CABLE, PIPE)) {
                for (Direction power : List.of(Direction.NORTH, Direction.SOUTH, Direction.DOWN)) {
                    for (boolean reactive : new boolean[]{false, true}) {
                        JsonObject line = new JsonObject();
                        JsonArray steps = new JsonArray();
                        GameTestPlayer[] player = new GameTestPlayer[1];
                        String name = transmitter.substring(transmitter.indexOf(':') + 1) + "/" + power.getSerializedName() + "/" + (reactive ? "reactive_first" : "plain_first");
                        sequence.thenExecuteAfter(1, () -> {
                            scene.reset(helper);
                            scene.place(transmitter, T.relative(Direction.EAST), Direction.NORTH);
                            scene.place(transmitter.equals(CABLE) ? MACHINE : TANK, T.relative(Direction.WEST), Direction.NORTH);
                            scene.place("chest", T.relative(Direction.UP), Direction.NORTH);
                            scene.place(transmitter, T, Direction.NORTH);
                            player[0] = (GameTestPlayer) helper.makeMockPlayerLookingAt(T, Direction.NORTH);
                            player[0].gameMode.changeGameModeForPlayer(GameType.SURVIVAL);
                            scene.clicker = player[0];
                            line.addProperty("case", name);
                        });
                        sequence.thenExecuteAfter(SETTLE, () -> record(scene, player[0], steps, "start", null));
                        String[] plan = reactive ? new String[]{"toggle", "power_on", "power_off", "power_on", "toggle", "power_off"} : new String[]{"power_on", "power_off", "toggle", "power_on", "toggle", "power_off"};
                        for (String step : plan) {
                            sequence.thenExecuteAfter(1, () -> {
                                switch (step) {
                                    case "power_on" -> scene.helper.setBlock(T.relative(power), Blocks.REDSTONE_BLOCK.defaultBlockState());
                                    case "power_off" -> scene.helper.setBlock(T.relative(power), Blocks.AIR.defaultBlockState());
                                    default -> click(scene, player[0], Direction.NORTH, Direction.NORTH, false);
                                }
                            });
                            sequence.thenExecuteAfter(4, () -> record(scene, player[0], steps, step, null));
                        }
                        sequence.thenExecuteAfter(1, () -> {
                            line.add("steps", steps);
                            scene.lines.add(line.toString());
                        });
                    }
                }
            }
            sequence.thenExecuteAfter(1, () -> write("transmitter_redstone", scene.lines));
            sequence.thenSucceed();
        });
    }

    private static void record(Scene scene, GameTestPlayer player, JsonArray steps, String step, String unused) {
        JsonObject o = new JsonObject();
        o.addProperty("step", step);
        o.add("t", scene.snap(T));
        o.add("east", scene.snap(T.relative(Direction.EAST)));
        o.add("overlay", drainOverlay(player));
        o.add("nbt", scene.nbt(T));
        steps.add(o);
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // NBT
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static CompoundTag compound(Object... pairs) {
        CompoundTag tag = new CompoundTag();
        for (int i = 0; i < pairs.length; i += 2) {
            String key = (String) pairs[i];
            Object value = pairs[i + 1];
            if (value instanceof Byte b) {
                tag.putByte(key, b);
            } else if (value instanceof Integer n) {
                tag.putInt(key, n);
            } else if (value instanceof int[] a) {
                tag.putIntArray(key, a);
            } else if (value instanceof String s) {
                tag.putString(key, s);
            } else if (value instanceof Double d) {
                tag.putDouble(key, d);
            } else if (value instanceof Long l) {
                tag.putLong(key, l);
            } else if (value instanceof Short s) {
                tag.putShort(key, s);
            }
        }
        return tag;
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records Transmitter.read and handleUpdateTag on fresh tiles for a set of tags.")
    public static void nbt(final DynamicTest test) {
        template(test);
        Scene scene = new Scene();
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  scene.reset(helper);
                  scene.lines.add("{\"meta\":\"commit " + COMMIT + ". Each case: a tag (typed: byte, int, string, int_array, double, long, short) read by Transmitter.read and by Transmitter.handleUpdateTag on a fresh tile "
                        + "without a level (client side by the null level). read and update: the state after (cur_t, cur_a, raw, reactive, has_pull, types) and for update the boolean it returned (refresh_model_data) and has_network.\"}");
                  List<Object[]> cases = new ArrayList<>();
                  cases.add(new Object[]{"empty", compound()});
                  cases.add(new Object[]{"typical", compound("redstone", (byte) 1, "connections", (byte) 5, "acceptors", (byte) 40, "connection", new int[]{0, 1, 2, 3, 0, 1})});
                  cases.add(new Object[]{"negative_bytes", compound("redstone", (byte) 1, "connections", (byte) -1, "acceptors", (byte) -128, "connection", new int[]{3, 3, 3, 3, 3, 3})});
                  cases.add(new Object[]{"ids_out_of_range", compound("connection", new int[]{-1, 4, 99, 3, 2, -7})});
                  cases.add(new Object[]{"short_array", compound("connection", new int[]{1, 2})});
                  cases.add(new Object[]{"long_array", compound("connection", new int[]{1, 1, 1, 1, 1, 1, 3, 3})});
                  cases.add(new Object[]{"empty_array", compound("connection", new int[]{})});
                  cases.add(new Object[]{"redstone_two", compound("redstone", (byte) 2)});
                  cases.add(new Object[]{"redstone_int", compound("redstone", 7, "connections", 300, "acceptors", 129)});
                  cases.add(new Object[]{"redstone_string", compound("redstone", "true", "connections", "5", "connection", "x")});
                  cases.add(new Object[]{"double_bytes", compound("connections", 5.9, "acceptors", 300.5, "redstone", 0.4)});
                  cases.add(new Object[]{"long_bytes", compound("connections", 258L, "acceptors", -1L, "redstone", 0L)});
                  cases.add(new Object[]{"short_bytes", compound("connections", (short) 257, "acceptors", (short) -2, "redstone", (short) 256)});
                  cases.add(new Object[]{"pull_all", compound("connection", new int[]{2, 2, 2, 2, 2, 2})});
                  cases.add(new Object[]{"byte_connection", compound("connection", (byte) 3)});
                  for (String id : List.of(CABLE, PIPE)) {
                      Block block = BuiltInRegistries.BLOCK.getValue(Identifier.parse(id));
                      for (Object[] c : cases) {
                          CompoundTag tag = (CompoundTag) c[1];
                          JsonObject line = new JsonObject();
                          line.addProperty("case", id.substring(id.indexOf(':') + 1) + "/" + c[0]);
                          line.add("tag", tagJson(tag));
                          for (String mode : List.of("read", "update")) {
                              BlockState state = block.defaultBlockState();
                              BlockEntity be = ((EntityBlock) block).newBlockEntity(scene.abs(T), state);
                              TileEntityTransmitter tile = (TileEntityTransmitter) be;
                              Transmitter<?, ?, ?> t = tile.getTransmitter();
                              var input = TagValueInput.create(ProblemReporter.DISCARDING, scene.level().registryAccess(), tag);
                              JsonObject o = new JsonObject();
                              if (mode.equals("read")) {
                                  t.read(input);
                              } else {
                                  o.addProperty("refresh_model_data", t.handleUpdateTag(input));
                              }
                              o.addProperty("cur_t", t.currentTransmitterConnections & 0xFF);
                              o.addProperty("cur_a", t.getAcceptorCache().currentAcceptorConnections & 0xFF);
                              JsonArray raw = new JsonArray();
                              JsonArray types = new JsonArray();
                              for (Direction side : SIDES) {
                                  raw.add(t.getConnectionTypeRaw(side).getSerializedName());
                                  types.add(t.getConnectionType(side).getSerializedName());
                              }
                              o.add("raw", raw);
                              o.add("types", types);
                              o.addProperty("reactive", flag(t, "redstoneReactive"));
                              o.addProperty("has_pull", flag(t, "hasPullSide"));
                              o.addProperty("has_network", t.hasTransmitterNetwork());
                              line.add(mode, o);
                          }
                          scene.lines.add(line.toString());
                      }
                  }
              })
              .thenExecuteAfter(1, () -> write("transmitter_nbt", scene.lines))
              .thenSucceed());
    }
}
