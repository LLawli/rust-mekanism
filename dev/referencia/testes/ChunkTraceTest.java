package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import java.util.function.Supplier;
import mekanism.api.functions.ConstantPredicates;
import mekanism.common.content.network.transmitter.Transmitter;
import mekanism.common.lib.transmitter.DynamicNetwork;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismTileEntityTypes;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.transmitter.TileEntityTransmitter;
import mekanism.common.tile.transmitter.TileEntityUniversalCable;
import mekanism.common.util.WorldUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ChunkHolder;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.DistanceManager;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate;
import net.minecraft.world.level.storage.ValueOutput;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.event.level.ChunkTicketLevelUpdatedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.RegisterStructureTemplate;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;
import org.jspecify.annotations.Nullable;

/// Grava a ordem em que o jogo dá os eventos de chunk nos dois cenários do `TransmitterNetworkTest` que mexem em chunks
/// (`reloadIntermediary` e `inaccessibleNotUnloaded`), para o simulador `sim::chunks` do Rust ser comparado com o jogo.
/// Cada linha do `.jsonl` é um evento, na ordem em que aconteceu, com o tick do servidor relativo ao início do cenário:
///
/// - `ticket`: `ChunkTicketLevelUpdatedEvent` de um dos três chunks da linha (`chunk` 0, 1 ou 2, relativo à estrutura);
/// - `load` e `unload`: `ChunkEvent.Load` e `ChunkEvent.Unload`, com a ordem em que o `LevelChunk` guarda os block entities;
/// - `added`, `onLoad`, `tick`, `saved`, `onChunkUnloaded` e `setRemoved`: o que o jogo chama em cada cabo (`x` é a posição
///   relativa), gravado por uma subclasse do `TileEntityUniversalCable` que entra no lugar da fábrica do tipo;
/// - `pre` e `post`: `ServerTickEvent.Pre` e `.Post`;
/// - `mark` e `state`: o que o teste está fazendo, e o estado dos três chunks naquele ponto.
///
/// Os cenários copiam os do Mekanism passo a passo. O arquivo é regravado a cada tick, para sobrar mesmo se o cenário falhar.
@ForEachTest(groups = "trace_chunks")
public class ChunkTraceTest {

    private static final String STRAIGHT_CABLE = MekanismTests.MODID + ":chunk_trace_straight_cable";
    private static final int CHUNKS = 3;
    private static final int TIMEOUT = 20 * SharedConstants.TICKS_PER_SECOND;

    //Note: lazy so the universal cable is registered, and so the traced block entity is in place before the structure is placed
    @RegisterStructureTemplate(STRAIGHT_CABLE)
    public static final Supplier<StructureTemplate> STRAIGHT_CABLE_TEMPLATE = StructureTemplateBuilder.lazy(CHUNKS * 16, 1, 1,
          builder -> {
              Recorder.installTracedCable();
              return builder.fill(0, 0, 0, CHUNKS * 16 - 1, 0, 0, MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState());
          }
    );

    @GameTest(template = STRAIGHT_CABLE, timeoutTicks = TIMEOUT, batch = "trace_chunks_reload")
    @TestHolder(description = "Records the chunk events of the reloadIntermediary scenario.")
    public static void chunkReloadIntermediary(final DynamicTest test) {
        final ChunkData chunkData = new ChunkData(1, 0);
        test.eventListeners().forge().addListener((final ChunkEvent.Unload event) -> chunkData.updateChunk(event, false));
        test.eventListeners().forge().addListener((final ChunkEvent.Load event) -> chunkData.updateChunk(event, true));
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenMap(() -> {
                  Recorder.begin("chunk_reload_intermediary", helper);
                  Recorder.mark("unload requested");
                  int level = chunkData.updateChunkLoading(helper, false, MekGameTestHelper.UNLOAD_LEVEL);
                  Recorder.mark("unload returned, level " + level);
                  Recorder.state(helper);
                  return level;
              })
              .thenWaitUntil(() -> {
                  chunkData.waitFor(helper, false);
                  Recorder.mark("saw unloaded");
              })
              .thenExecuteAfter(5, level -> {
                  Recorder.mark("load requested");
                  chunkData.updateChunkLoading(helper, true, level);
                  Recorder.mark("load returned");
                  Recorder.state(helper);
              })
              .thenWaitUntil(() -> {
                  chunkData.waitFor(helper, true);
                  Recorder.mark("saw loaded");
              })
              .thenExecuteAfter(5, () -> {
                  Recorder.mark("validate");
                  Recorder.state(helper);
                  new MatchingNetworkValidator(helper).run();
              })
              .thenExecute(Recorder::finish)
              .thenSucceed()
        );
    }

    @GameTest(template = STRAIGHT_CABLE, timeoutTicks = TIMEOUT, batch = "trace_chunks_inaccessible")
    @TestHolder(description = "Records the chunk events of the inaccessibleNotUnloaded scenario.")
    public static void chunkInaccessibleNotUnloaded(final DynamicTest test) {
        listen(test);
        final ChunkPos relativeChunk = new ChunkPos(1, 0);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenMap(() -> {
                  Recorder.begin("chunk_inaccessible_not_unloaded", helper);
                  Recorder.mark("set level inaccessible");
                  int level = helper.setChunkLoadLevel(relativeChunk, MekGameTestHelper.INACCESSIBLE_LEVEL);
                  Recorder.mark("set level returned, old level " + level);
                  Recorder.state(helper);
                  return level;
              })
              .thenIdle(5)
              .thenSequence(sequence -> sequence.thenMap(() -> helper.getBlockState(0, 0, 0))
                    .thenExecute(() -> {
                        Recorder.mark("break cable 0");
                        helper.setBlock(0, 0, 0, Blocks.AIR);
                        Recorder.mark("break cable 0 returned");
                    })
                    .thenExecuteAfter(5, (BlockState state) -> {
                        Recorder.mark("place cable 0");
                        helper.setBlock(0, 0, 0, state);
                        Recorder.mark("place cable 0 returned");
                    })
              )
              .thenExecuteAfter(5, level -> {
                  Recorder.mark("set level back " + level);
                  helper.setChunkLoadLevel(relativeChunk, level);
                  Recorder.mark("set level back returned");
                  Recorder.state(helper);
              })
              .thenExecuteAfter(5, () -> {
                  Recorder.mark("validate");
                  Recorder.state(helper);
                  new MatchingNetworkValidator(helper).run();
              })
              .thenExecute(Recorder::finish)
              .thenSucceed()
        );
    }

    private static void listen(DynamicTest test) {
        test.eventListeners().forge().addListener((final ChunkTicketLevelUpdatedEvent event) -> Recorder.ticket(event));
        test.eventListeners().forge().addListener((final ChunkEvent.Load event) -> Recorder.chunk(event, "load"));
        test.eventListeners().forge().addListener((final ChunkEvent.Unload event) -> Recorder.chunk(event, "unload"));
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> Recorder.tickEvent("pre"));
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> {
            Recorder.tickEvent("post");
            Recorder.write();
        });
    }

    /// The cable block entity of the straight line, with a line for every call the game makes on it.
    private static class TracedCable extends TileEntityUniversalCable {

        TracedCable(BlockPos pos, BlockState state) {
            super(MekanismBlocks.BASIC_UNIVERSAL_CABLE, pos, state);
        }

        @Override
        public void clearRemoved() {
            Recorder.cable("added", getBlockPos());
            super.clearRemoved();
        }

        @Override
        public void onLoad() {
            Recorder.cable("onLoad", getBlockPos());
            super.onLoad();
        }

        @Override
        protected void onUpdateServer(ServerLevel level) {
            Recorder.cable("tick", getBlockPos());
            super.onUpdateServer(level);
        }

        @Override
        public void saveAdditional(ValueOutput output) {
            Recorder.cable("saved", getBlockPos());
            super.saveAdditional(output);
        }

        @Override
        public void onChunkUnloaded() {
            Recorder.cable("onChunkUnloaded", getBlockPos());
            super.onChunkUnloaded();
        }

        @Override
        public void setRemoved() {
            Recorder.cable("setRemoved", getBlockPos());
            super.setRemoved();
        }
    }

    private static class Recorder {

        @Nullable
        private static volatile Recorder current;
        private static boolean installed;

        private final String name;
        private final List<String> lines = new ArrayList<>();
        private final ServerLevel level;
        private final BlockPos origin;
        private final ChunkPos originChunk;
        private final long startTick;
        private int written = -1;

        private Recorder(String name, MekGameTestHelper helper) {
            this.name = name;
            this.level = helper.getLevel();
            this.origin = helper.absolutePos(BlockPos.ZERO);
            this.originChunk = ChunkPos.containing(origin);
            this.startTick = ServerLifecycleHooks.getCurrentServer().getTickCount();
        }

        /// Puts [TracedCable] in place of the factory of the cable block entity type, as a no-op while nothing is recording.
        static synchronized void installTracedCable() {
            if (installed) {
                return;
            }
            try {
                BlockEntityType<TileEntityUniversalCable> type = MekanismTileEntityTypes.BASIC_UNIVERSAL_CABLE.get();
                Field factory = BlockEntityType.class.getDeclaredField("factory");
                factory.setAccessible(true);
                BlockEntityType.BlockEntitySupplier<TileEntityUniversalCable> traced = TracedCable::new;
                factory.set(type, traced);
                installed = true;
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        static void begin(String name, MekGameTestHelper helper) {
            current = new Recorder(name, helper);
            Recorder recorder = current;
            //The geometry: where the structure sits in its chunks, and which chunk the Mekanism scenario calls (1, 0)
            ChunkPos middle = helper.absolutePos(new ChunkPos(1, 0));
            recorder.add("origin", "\"x\":" + recorder.origin.getX() + ",\"y\":" + recorder.origin.getY() + ",\"z\":" + recorder.origin.getZ()
                                   + ",\"xInChunk\":" + (recorder.origin.getX() & 15) + ",\"zInChunk\":" + (recorder.origin.getZ() & 15)
                                   + ",\"middleCx\":" + (middle.x() - recorder.originChunk.x()) + ",\"middleCz\":" + (middle.z() - recorder.originChunk.z()));
            mark("start");
            state(helper);
        }

        static synchronized void finish() {
            Recorder recorder = current;
            if (recorder != null) {
                recorder.add("mark", "\"m\":\"finish\"");
                recorder.flush();
                current = null;
            }
        }

        static synchronized void write() {
            Recorder recorder = current;
            if (recorder != null) {
                recorder.flush();
            }
        }

        static void mark(String text) {
            add(recorder -> recorder.add("mark", "\"m\":\"" + text + "\""));
        }

        static void tickEvent(String kind) {
            add(recorder -> recorder.add(kind, ""));
        }

        static void cable(String kind, BlockPos pos) {
            add(recorder -> {
                int x = pos.getX() - recorder.origin.getX();
                //Only the cables of this structure: other tests run at the same time, with cables of their own
                if (x >= 0 && x < CHUNKS * 16 && pos.getY() == recorder.origin.getY() && pos.getZ() == recorder.origin.getZ()) {
                    recorder.add(kind, "\"x\":" + x);
                }
            });
        }

        static void ticket(ChunkTicketLevelUpdatedEvent event) {
            add(recorder -> {
                if (event.getLevel() != recorder.level) {
                    return;
                }
                ChunkPos pos = ChunkPos.unpack(event.getChunkPos());
                String chunk = recorder.relativeChunk(pos);
                if (chunk != null) {
                    recorder.add("ticket", chunk + ",\"old\":" + event.getOldTicketLevel() + ",\"new\":" + event.getNewTicketLevel());
                }
            });
        }

        static void chunk(ChunkEvent<LevelChunk> event, String kind) {
            add(recorder -> {
                if (event.getLevel() != recorder.level) {
                    return;
                }
                LevelChunk chunk = event.getChunk();
                String relative = recorder.relativeChunk(chunk.getPos());
                if (relative == null) {
                    return;
                }
                StringBuilder order = new StringBuilder();
                for (BlockPos pos : chunk.getBlockEntities().keySet()) {
                    order.append(order.isEmpty() ? "" : ",").append(pos.getX() - recorder.origin.getX());
                }
                String extra = event instanceof ChunkEvent.Load load ? ",\"new\":" + load.isNewChunk() : "";
                recorder.add(kind, relative + extra + ",\"blockEntities\":[" + order + "]");
            });
        }

        /// The state of the three chunks of the line, as the scenario sees it.
        static void state(MekGameTestHelper helper) {
            add(recorder -> {
                ChunkMap chunkMap = helper.getChunkMap();
                DistanceManager distanceManager = chunkMap.getDistanceManager();
                StringBuilder chunks = new StringBuilder();
                for (int x = 0; x <= CHUNKS; x++) {
                    for (int z = -1; z <= 1; z++) {
                        ChunkPos absolute = new ChunkPos(recorder.originChunk.x() + x, recorder.originChunk.z() + z);
                        ChunkHolder holder = distanceManager.getChunk(absolute.pack());
                        if (holder == null && !WorldUtils.isChunkLoaded(recorder.level, absolute)) {
                            continue;
                        }
                        chunks.append(chunks.isEmpty() ? "" : ",").append("{\"cx\":").append(x).append(",\"cz\":").append(z)
                              .append(",\"hasChunk\":").append(WorldUtils.isChunkLoaded(recorder.level, absolute))
                              .append(",\"level\":").append(holder == null ? "null" : Integer.toString(holder.getTicketLevel())).append('}');
                    }
                }
                recorder.add("state", "\"chunks\":[" + chunks + "]");
            });
        }

        private static synchronized void add(java.util.function.Consumer<Recorder> action) {
            Recorder recorder = current;
            if (recorder != null) {
                action.accept(recorder);
            }
        }

        /// The chunk as offsets from the chunk of the structure origin, or `null` when it is not near the line.
        private @Nullable String relativeChunk(ChunkPos pos) {
            int x = pos.x() - originChunk.x();
            int z = pos.z() - originChunk.z();
            return x >= 0 && x <= CHUNKS && z >= -1 && z <= 1 ? "\"cx\":" + x + ",\"cz\":" + z : null;
        }

        private void add(String kind, String fields) {
            long tick = ServerLifecycleHooks.getCurrentServer().getTickCount() - startTick;
            lines.add("{\"t\":" + tick + ",\"e\":\"" + kind + "\"" + (fields.isEmpty() ? "" : "," + fields) + "}");
        }

        private void flush() {
            if (written == lines.size()) {
                return;
            }
            try {
                Path file = Path.of("trace", name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
                written = lines.size();
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }

    private static class MatchingNetworkValidator implements Runnable {

        private final MekGameTestHelper helper;
        @Nullable
        private UUID networkUUID;

        MatchingNetworkValidator(MekGameTestHelper helper) {
            this.helper = helper;
        }

        @Override
        public void run() {
            helper.forEveryBlockInStructure(relativePos -> {
                if (helper.isBlockLoaded(relativePos)) {
                    Transmitter<?, ?, ?> transmitter = helper.getBlockEntity(relativePos, TileEntityTransmitter.class).getTransmitter();
                    if (!transmitter.hasTransmitterNetwork()) {
                        helper.fail("No transmitter network found", relativePos);
                    }
                    DynamicNetwork<?, ?, ?> network = transmitter.getTransmitterNetworkNN();
                    if (networkUUID == null) {
                        networkUUID = network.getUUID();
                    } else if (!networkUUID.equals(network.getUUID())) {
                        helper.fail("Multiple transmitter networks", relativePos);
                    }
                } else {
                    helper.fail("Expected expected position to be loaded", relativePos);
                }
            });
        }
    }

    /// The `ChunkData` of the Mekanism test, without the debug logging.
    private static class ChunkData {

        private final ChunkPos relativePos;
        @Nullable
        private ChunkPos absolutePos;
        private long absPos;
        private boolean isLoaded = true;

        ChunkData(int x, int y) {
            this.relativePos = new ChunkPos(x, y);
        }

        int updateChunkLoading(MekGameTestHelper helper, boolean load, int level) {
            if (absolutePos == null) {
                absolutePos = helper.absolutePos(relativePos);
                absPos = absolutePos.pack();
            }
            if (helper.isChunkLoaded(relativePos) != load) {
                ChunkMap chunkMap = helper.getChunkMap();
                DistanceManager distanceManager = chunkMap.getDistanceManager();
                ChunkHolder holder = distanceManager.getChunk(absPos);
                if ((holder == null) == load) {
                    isLoaded = !load;
                    if (load) {
                        holder = distanceManager.updateChunkScheduling(absPos, level, holder, MekGameTestHelper.UNLOAD_LEVEL);
                        if (holder == null) {
                            helper.fail("Error loading chunk", relativePos);
                        } else {
                            distanceManager.chunksToUpdateFutures.add(holder);
                        }
                    } else {
                        level = holder.getTicketLevel();
                        distanceManager.updateChunkScheduling(absPos, MekGameTestHelper.UNLOAD_LEVEL, holder, level);
                        chunkMap.processUnloads(ConstantPredicates.ALWAYS_TRUE);
                    }
                } else {
                    Recorder.mark("updateChunkLoading skipped: holder " + (holder == null ? "absent" : "present"));
                }
            } else {
                Recorder.mark("updateChunkLoading skipped: chunk already " + (load ? "loaded" : "unloaded"));
            }
            return level;
        }

        void updateChunk(ChunkEvent<LevelChunk> event, boolean loaded) {
            if (!event.getLevel().isClientSide() && event.getChunk().getPos().equals(absolutePos)) {
                isLoaded = loaded;
            }
        }

        void waitFor(MekGameTestHelper helper, boolean loaded) {
            if (isLoaded != loaded) {
                helper.fail("Chunk has not been marked as " + (loaded ? "loaded" : "unloaded") + " yet", relativePos);
            }
        }
    }
}
