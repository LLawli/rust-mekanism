package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.function.Supplier;
import mekanism.api.functions.ConstantPredicates;
import mekanism.common.capabilities.Capabilities;
import mekanism.common.content.network.EnergyNetwork;
import mekanism.common.content.network.transmitter.UniversalCable;
import mekanism.common.lib.transmitter.DynamicNetwork;
import mekanism.common.lib.transmitter.TransmitterNetworkRegistry;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismTileEntityTypes;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.transmitter.TileEntityUniversalCable;
import mekanism.common.util.WorldUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.server.level.ChunkHolder;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.DistanceManager;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate;
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

/// Grava a topologia da rede de transmissores (frente `rede-registro`, Fase 3), um `.jsonl` por cenário em `trace/network_<cenário>.jsonl`, para
/// `mods/machinery_basis/core/tests/transmission_registry.rs` repetir. Cada linha é um evento com o tick do servidor relativo ao início da gravação
/// (`t`):
///
/// - `setup`: a origem da estrutura (`x`, `y`, `z` absolutos) e o `identityHashCode` do `Transmitter` de cada cabo que já existe (`tiles`), porque a
///   ordem de iteração dos `ObjectOpenHashSet` de transmissores depende dele e o JVM o sorteia a cada execução;
/// - `tile`: um cabo novo (o `identityHashCode` dele);
/// - `mark`: a ação do cenário (`place N`, `break N`, `unload C`, `load C`, `level C L`, `freeze` e `unfreeze`, com o prefixo `pre ` quando roda
///   antes do tick do mundo, no `ServerTickEvent.Pre`);
/// - `created`: uma rede nova (`net` é o índice de criação, `hash` o `UUID.hashCode()`, que decide a ordem do `networks` e do `networksToChange`, e
///   `from` as redes que ela adotou); `update` (`requestsUpdate` de um cabo), `commit`, `invalidate` e `net_update` (o `onUpdate` da rede), na ordem
///   em que acontecem; `access`: o `chunkAccessibilityChange` de um cabo;
/// - `state`: depois de `TransmitterNetworkRegistry.onTick`, cada cabo (`net`, `orphan`, `valid`, `loaded`), cada rede (tamanho, aceitadores, `hash` e os
///   transmissores na ordem do `Long2ObjectOpenHashMap`) e a ordem do `networks` do registro (`registry`).
///
/// A rede sai como o índice de criação e o UUID não é gravado: os testes em Rust dão os mesmos hashes. Copie o arquivo para
/// `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace_network")
public class NetworkTopologyTraceTest {

    private static final String LINE5 = MekanismTests.MODID + ":network_trace_line5";
    private static final String EMPTY = MekanismTests.MODID + ":network_trace_empty";
    private static final String GAP = MekanismTests.MODID + ":network_trace_gap";
    private static final String TWO_GAPS = MekanismTests.MODID + ":network_trace_two_gaps";
    private static final String ACCEPTOR_LINE = MekanismTests.MODID + ":network_trace_acceptor_line";
    private static final String GRID = MekanismTests.MODID + ":network_trace_grid";
    private static final String STRAIGHT_48 = MekanismTests.MODID + ":network_trace_straight_48";
    private static final int CHUNKS = 3;
    private static final int TIMEOUT = 30 * SharedConstants.TICKS_PER_SECOND;
    private static final int SETUP = SharedConstants.TICKS_PER_SECOND;

    //Note: lazy so the universal cable is registered, and so the traced block entity is in place before the structure is placed
    @RegisterStructureTemplate(LINE5)
    public static final Supplier<StructureTemplate> LINE5_TEMPLATE = StructureTemplateBuilder.lazy(8, 1, 1, builder -> {
        Recorder.installTracedCable();
        return builder.fill(0, 0, 0, 4, 0, 0, MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState());
    });

    @RegisterStructureTemplate(EMPTY)
    public static final Supplier<StructureTemplate> EMPTY_TEMPLATE = StructureTemplateBuilder.lazy(8, 1, 1, builder -> {
        Recorder.installTracedCable();
        return builder.fill(7, 0, 0, 7, 0, 0, Blocks.AIR.defaultBlockState());
    });

    @RegisterStructureTemplate(GAP)
    public static final Supplier<StructureTemplate> GAP_TEMPLATE = StructureTemplateBuilder.lazy(8, 1, 1, builder -> {
        Recorder.installTracedCable();
        return builder.fill(0, 0, 0, 1, 0, 0, MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState())
              .fill(3, 0, 0, 4, 0, 0, MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState());
    });

    @RegisterStructureTemplate(TWO_GAPS)
    public static final Supplier<StructureTemplate> TWO_GAPS_TEMPLATE = StructureTemplateBuilder.lazy(8, 1, 1, builder -> {
        Recorder.installTracedCable();
        return builder.fill(0, 0, 0, 1, 0, 0, MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState())
              .fill(5, 0, 0, 6, 0, 0, MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState());
    });

    @RegisterStructureTemplate(ACCEPTOR_LINE)
    public static final Supplier<StructureTemplate> ACCEPTOR_LINE_TEMPLATE = StructureTemplateBuilder.lazy(8, 1, 2, builder -> {
        Recorder.installTracedCable();
        return builder.fill(0, 0, 0, 4, 0, 0, MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState());
    });

    @RegisterStructureTemplate(GRID)
    public static final Supplier<StructureTemplate> GRID_TEMPLATE = StructureTemplateBuilder.lazy(8, 1, 7, builder -> {
        Recorder.installTracedCable();
        return builder.fill(7, 0, 0, 7, 0, 0, Blocks.AIR.defaultBlockState());
    });

    @RegisterStructureTemplate(STRAIGHT_48)
    public static final Supplier<StructureTemplate> STRAIGHT_48_TEMPLATE = StructureTemplateBuilder.lazy(CHUNKS * 16, 1, 1, builder -> {
        Recorder.installTracedCable();
        return builder.fill(0, 0, 0, CHUNKS * 16 - 1, 0, 0, MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState());
    });

    @GameTest(template = EMPTY, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_network_line")
    @TestHolder(description = "Records a line of cables being placed, extended at the end and then at the start.")
    public static void line(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> Recorder.begin("line", helper, 8))
              .thenExecuteAfter(2, () -> place(helper, 1, 2, 3, 4, 5))
              .thenExecuteAfter(5, () -> place(helper, 6))
              .thenExecuteAfter(5, () -> place(helper, 0))
              .thenExecuteAfter(5, Recorder::finish)
              .thenSucceed()
        );
    }

    @GameTest(template = GAP, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_network_merge")
    @TestHolder(description = "Records a cable that joins two networks.")
    public static void merge(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> Recorder.begin("merge", helper, 8))
              .thenExecuteAfter(2, () -> place(helper, 2))
              .thenExecuteAfter(8, Recorder::finish)
              .thenSucceed()
        );
    }

    @GameTest(template = LINE5, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_network_split")
    @TestHolder(description = "Records the middle cable of a line being broken (the network splits in two) and put back.")
    public static void splitMiddle(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> Recorder.begin("split_middle", helper, 8))
              .thenExecuteAfter(2, () -> breakCable(helper, 2))
              .thenExecuteAfter(8, () -> place(helper, 2))
              .thenExecuteAfter(8, Recorder::finish)
              .thenSucceed()
        );
    }

    @GameTest(template = LINE5, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_network_end")
    @TestHolder(description = "Records the end cables of a line being broken one after the other.")
    public static void breakEnd(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> Recorder.begin("break_end", helper, 8))
              .thenExecuteAfter(2, () -> breakCable(helper, 4))
              .thenExecuteAfter(5, () -> breakCable(helper, 0))
              .thenExecuteAfter(5, () -> breakCable(helper, 2))
              .thenExecuteAfter(5, Recorder::finish)
              .thenSucceed()
        );
    }

    @GameTest(template = TWO_GAPS, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_network_same_tick")
    @TestHolder(description = "Records two cables placed in the same tick, each next to a different network, and the cable that joins them.")
    public static void sameTick(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> Recorder.begin("same_tick", helper, 8))
              .thenExecuteAfter(2, () -> place(helper, 2, 4))
              .thenExecuteAfter(8, () -> place(helper, 3))
              .thenExecuteAfter(8, Recorder::finish)
              .thenSucceed()
        );
    }

    @GameTest(template = ACCEPTOR_LINE, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_network_acceptors")
    @TestHolder(description = "Records the acceptors of a network while energy cubes are placed next to it and the line is split.")
    public static void acceptors(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> Recorder.begin("acceptors", helper, 8))
              .thenExecuteAfter(2, () -> placeCube(helper, 1))
              .thenExecuteAfter(5, () -> placeCube(helper, 3))
              .thenExecuteAfter(5, () -> breakCable(helper, 2))
              .thenExecuteAfter(8, () -> breakCable(helper, 1))
              .thenExecuteAfter(8, () -> placeCube(helper, 4))
              .thenExecuteAfter(8, Recorder::finish)
              .thenSucceed()
        );
    }

    /// The chunk that holds the cables around the relative x `16 * index + 8`. The chunk of `MekGameTestHelper.absolutePos(ChunkPos)` takes the
    /// middle block at z = 8, which is another chunk of z when the structure sits at z % 16 >= 8, and then the line is not in it.
    private static ChunkPos lineChunk(MekGameTestHelper helper, int index) {
        return ChunkPos.containing(helper.absolutePos(new BlockPos(16 * index + 8, 0, 0)));
    }

    /// The cables of a 7 by 7 plane (indexes `z * 8 + x`), from the index `from` to `to`.
    private static int[] plane(int from, int to) {
        List<Integer> indexes = new ArrayList<>();
        for (int z = 0; z < 7; z++) {
            for (int x = 0; x < 7; x++) {
                if (z * 8 + x >= from && z * 8 + x <= to) {
                    indexes.add(z * 8 + x);
                }
            }
        }
        return indexes.stream().mapToInt(Integer::intValue).toArray();
    }

    @GameTest(template = GRID, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_network_grid")
    @TestHolder(description = "Records a plane of cables placed in two steps (a branching search, and a network that keeps growing) and its center broken.")
    public static void grid(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> Recorder.begin("grid", helper, 8, 7))
              .thenExecuteAfter(2, () -> place(helper, 8, plane(0, 33)))
              .thenExecuteAfter(5, () -> place(helper, 8, plane(34, 54)))
              .thenExecuteAfter(5, () -> breakCable(helper, 8, 27))
              .thenExecuteAfter(8, Recorder::finish)
              .thenSucceed()
        );
    }

    @GameTest(template = STRAIGHT_48, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_network_chunk_flip")
    @TestHolder(description = "Records the middle chunk of a line going inaccessible and back in the same step.")
    public static void chunkFlip(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("chunk_flip", helper, CHUNKS * 16);
                  ChunkPos chunk = lineChunk(helper, 1);
                  Recorder.mark("level 1 " + MekGameTestHelper.INACCESSIBLE_LEVEL);
                  int level = setChunkLoadLevel(helper, chunk, MekGameTestHelper.INACCESSIBLE_LEVEL);
                  Recorder.mark("level 1 " + level);
                  setChunkLoadLevel(helper, chunk, level);
              })
              .thenExecuteAfter(8, Recorder::finish)
              .thenSucceed()
        );
    }

    @GameTest(template = STRAIGHT_48, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_network_chunk_reload")
    @TestHolder(description = "Records the network while the middle chunk of a line is unloaded and loaded again.")
    public static void chunkReload(final DynamicTest test) {
        final ChunkData chunkData = new ChunkData(1);
        test.eventListeners().forge().addListener((final ChunkEvent.Unload event) -> chunkData.updateChunk(event, false));
        test.eventListeners().forge().addListener((final ChunkEvent.Load event) -> chunkData.updateChunk(event, true));
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> Recorder.begin("chunk_reload", helper, CHUNKS * 16))
              .thenMap(() -> {
                  Recorder.mark("unload 1");
                  return chunkData.updateChunkLoading(helper, false, MekGameTestHelper.UNLOAD_LEVEL);
              })
              .thenWaitUntil(() -> chunkData.waitFor(helper, false))
              .thenExecuteAfter(5, level -> {
                  Recorder.mark("load 1");
                  chunkData.updateChunkLoading(helper, true, level);
              })
              .thenWaitUntil(() -> chunkData.waitFor(helper, true))
              .thenExecuteAfter(8, Recorder::finish)
              .thenSucceed()
        );
    }

    @GameTest(template = STRAIGHT_48, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_network_chunk_inaccessible")
    @TestHolder(description = "Records the network while the middle chunk of a line is inaccessible but not unloaded, with a cable broken meanwhile.")
    public static void chunkInaccessible(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenMap(() -> {
                  Recorder.begin("chunk_inaccessible", helper, CHUNKS * 16);
                  Recorder.mark("level 1 " + MekGameTestHelper.INACCESSIBLE_LEVEL);
                  return setChunkLoadLevel(helper, lineChunk(helper, 1), MekGameTestHelper.INACCESSIBLE_LEVEL);
              })
              .thenIdle(5)
              .thenSequence(sequence -> sequence.thenMap(() -> helper.getBlockState(0, 0, 0))
                    .thenExecute(() -> breakCable(helper, 0))
                    .thenExecuteAfter(5, (BlockState state) -> {
                        Recorder.mark("place 0");
                        helper.setBlock(0, 0, 0, state);
                    })
              )
              .thenExecuteAfter(5, level -> {
                  Recorder.mark("level 1 " + level);
                  setChunkLoadLevel(helper, lineChunk(helper, 1), level);
              })
              .thenExecuteAfter(8, Recorder::finish)
              .thenSucceed()
        );
    }

    @GameTest(template = LINE5, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_network_frozen")
    @TestHolder(description = "Records the registry working while the tick rate manager is frozen: a cable broken meanwhile, and no onUpdate.")
    public static void frozen(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("frozen", helper, 8);
                  //Actions run in ServerTickEvent.Pre, because the game test sequence does not advance while the server is frozen
                  Recorder.schedule(3, "pre freeze", () -> ServerLifecycleHooks.getCurrentServer().tickRateManager().setFrozen(true));
                  Recorder.schedule(6, "pre break 2", () -> helper.setBlock(2, 0, 0, Blocks.AIR));
                  Recorder.schedule(14, "pre unfreeze", () -> ServerLifecycleHooks.getCurrentServer().tickRateManager().setFrozen(false));
                  Recorder.schedule(20, "pre finish", Recorder::finish);
              })
              .thenWaitUntil(() -> {
                  if (!Recorder.isFinished()) {
                      helper.fail("Trace not finished yet");
                  }
              })
              .thenSucceed()
        );
    }

    /// `MekGameTestHelper.setChunkLoadLevel` for an absolute chunk.
    private static int setChunkLoadLevel(MekGameTestHelper helper, ChunkPos chunk, int newLevel) {
        DistanceManager distanceManager = helper.getChunkMap().getDistanceManager();
        ChunkHolder holder = distanceManager.getChunk(chunk.pack());
        int oldLevel = holder == null ? MekGameTestHelper.UNLOAD_LEVEL : holder.getTicketLevel();
        distanceManager.updateChunkScheduling(chunk.pack(), newLevel, holder, oldLevel);
        return oldLevel;
    }

    private static void place(MekGameTestHelper helper, int... indexes) {
        place(helper, 8, indexes);
    }

    /// Places the cables with the given indexes (`z * width + x`).
    private static void place(MekGameTestHelper helper, int width, int[] indexes) {
        for (int index : indexes) {
            Recorder.mark("place " + index);
            helper.setBlock(index % width, 0, index / width, MekanismBlocks.BASIC_UNIVERSAL_CABLE.defaultState());
        }
    }

    private static void breakCable(MekGameTestHelper helper, int x) {
        breakCable(helper, 8, x);
    }

    private static void breakCable(MekGameTestHelper helper, int width, int index) {
        Recorder.mark("break " + index);
        helper.setBlock(index % width, 0, index / width, Blocks.AIR);
    }

    private static void placeCube(MekGameTestHelper helper, int x) {
        //The front of the machine faces away from the cable: the other sides accept energy
        Recorder.mark("cube " + x);
        helper.setBlock(x, 0, 1, MekanismBlocks.ENRICHMENT_CHAMBER.defaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, Direction.SOUTH));
    }

    private static void listen(DynamicTest test) {
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> Recorder.pre());
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> Recorder.post());
        test.eventListeners().forge().addListener((final ChunkEvent.Load event) -> Recorder.chunk("load", event));
        test.eventListeners().forge().addListener((final ChunkEvent.Unload event) -> Recorder.chunk("unload", event));
        test.eventListeners().forge().addListener((final ChunkTicketLevelUpdatedEvent event) -> Recorder.ticket(event));
    }

    /// The cable block entity, with a transmitter that logs `requestsUpdate` and creates the traced network.
    private static class TracedCable extends TileEntityUniversalCable {

        TracedCable(BlockPos pos, BlockState state) {
            super(MekanismBlocks.BASIC_UNIVERSAL_CABLE, pos, state);
            Recorder.tile(pos, System.identityHashCode(getTransmitter()));
        }

        @Override
        protected UniversalCable createTransmitter(Holder<Block> blockProvider) {
            return new TracedTransmitter(blockProvider, this);
        }

        @Override
        public void chunkAccessibilityChange(boolean loaded) {
            Recorder.log("access", getBlockPos(), "\"loaded\":" + loaded);
            super.chunkAccessibilityChange(loaded);
        }
    }

    private static class TracedTransmitter extends UniversalCable {

        TracedTransmitter(Holder<Block> blockProvider, TracedCable tile) {
            super(blockProvider, tile);
        }

        @Override
        public EnergyNetwork createNetworkByMerging(Collection<EnergyNetwork> networks) {
            return new TracedNetwork(networks);
        }

        @Override
        public void requestsUpdate() {
            Recorder.log("update", getBlockPos(), "");
            super.requestsUpdate();
        }
    }

    private static class TracedNetwork extends EnergyNetwork {

        private final int index;

        TracedNetwork(Collection<EnergyNetwork> from) {
            super(from);
            index = Recorder.nextNetworkIndex();
            StringBuilder merged = new StringBuilder();
            for (EnergyNetwork network : from) {
                merged.append(merged.isEmpty() ? "" : ",").append(network instanceof TracedNetwork traced ? traced.index : -1);
            }
            Recorder.event("created", "\"net\":" + index + ",\"hash\":" + getUUID().hashCode() + ",\"from\":[" + merged + "]");
        }

        @Override
        public void onUpdate() {
            Recorder.event("net_update", "\"net\":" + index);
            super.onUpdate();
        }

        @Override
        public void commit() {
            Recorder.event("commit", "\"net\":" + index);
            super.commit();
        }

        @Override
        public void invalidate(@Nullable UniversalCable triggerTransmitter) {
            Recorder.event("invalidate", "\"net\":" + index);
            super.invalidate(triggerTransmitter);
        }
    }

    private static class Recorder {

        @Nullable
        private static volatile Recorder current;
        private static boolean installed;
        private static int networks;

        private final String name;
        private final List<String> lines = new ArrayList<>();
        private final List<Scheduled> scheduled = new ArrayList<>();
        private final ServerLevel level;
        private final BlockPos origin;
        private final ChunkPos originChunk;
        private final int width;
        private final int depth;
        private final long startTick;
        private boolean finished;

        private Recorder(String name, MekGameTestHelper helper, int width, int depth) {
            this.name = name;
            this.level = helper.getLevel();
            this.origin = helper.absolutePos(BlockPos.ZERO);
            this.originChunk = ChunkPos.containing(origin);
            this.width = width;
            this.depth = depth;
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

        static synchronized int nextNetworkIndex() {
            return networks++;
        }

        /// What happened before `begin`, for the structure being built: tiles and networks, with the absolute position of the cables.
        private record Before(long tick, String kind, @Nullable BlockPos pos, String fields) {
        }

        private static final List<Before> BEFORE = new ArrayList<>();

        static synchronized void before(String kind, @Nullable BlockPos pos, String fields) {
            BEFORE.add(new Before(ServerLifecycleHooks.getCurrentServer().getTickCount(), kind, pos, fields));
            if (BEFORE.size() > 20000) {
                BEFORE.subList(0, 5000).clear();
            }
        }

        static void begin(String name, MekGameTestHelper helper, int width) {
            begin(name, helper, width, 1);
        }

        /// A line (`depth` 1) or a plane of cables; the cable at `x`, `z` has the index `z * width + x`.
        static void begin(String name, MekGameTestHelper helper, int width, int depth) {
            Recorder recorder = new Recorder(name, helper, width, depth);
            StringBuilder tiles = new StringBuilder();
            for (int x = 0; x < width * depth; x++) {
                TileEntityUniversalCable tile = recorder.cable(x);
                if (tile != null) {
                    tiles.append(tiles.isEmpty() ? "" : ",").append("{\"x\":").append(x).append(",\"hash\":")
                          .append(System.identityHashCode(tile.getTransmitter())).append('}');
                }
            }
            recorder.add("setup", "\"x\":" + recorder.origin.getX() + ",\"y\":" + recorder.origin.getY() + ",\"z\":" + recorder.origin.getZ()
                                  + ",\"width\":" + width + ",\"depth\":" + depth + ",\"xInChunk\":" + (recorder.origin.getX() & 15) + ",\"tiles\":[" + tiles + "]");
            current = recorder;
            //How the structure was built, as `before_*` events with the tick relative to `begin` (negative), cables of the line and every network
            synchronized (Recorder.class) {
                for (Before before : BEFORE) {
                    String fields = before.fields();
                    if (before.pos() != null) {
                        int x = recorder.index(before.pos());
                        if (x < 0) {
                            continue;
                        }
                        fields = "\"x\":" + x + (fields.isEmpty() ? "" : "," + fields);
                    }
                    recorder.lines.add("{\"t\":" + (before.tick() - recorder.startTick) + ",\"e\":\"before_" + before.kind() + "\"" + (fields.isEmpty() ? "" : "," + fields) + "}");
                }
            }
            recorder.recordState();
        }

        static synchronized void finish() {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished) {
                recorder.finished = true;
                recorder.flush();
            }
        }

        static boolean isFinished() {
            Recorder recorder = current;
            return recorder != null && recorder.finished;
        }

        static void schedule(int tick, String mark, Runnable action) {
            Recorder recorder = current;
            if (recorder != null) {
                recorder.scheduled.add(new Scheduled(tick, mark, action));
            }
        }

        static void pre() {
            Recorder recorder = current;
            if (recorder == null || recorder.finished) {
                return;
            }
            long tick = recorder.relativeTick();
            for (Scheduled task : new ArrayList<>(recorder.scheduled)) {
                if (task.tick == tick) {
                    recorder.scheduled.remove(task);
                    recorder.add("mark", "\"m\":\"" + task.mark + "\"");
                    task.action.run();
                }
            }
        }

        static void post() {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished) {
                recorder.recordState();
            }
        }

        static void mark(String text) {
            Recorder recorder = current;
            if (recorder != null) {
                recorder.add("mark", "\"m\":\"" + text + "\"");
            }
        }

        static void event(String kind, String fields) {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished) {
                recorder.add(kind, fields);
            } else if (!kind.equals("net_update")) {
                before(kind, null, fields);
            }
        }

        /// `ChunkEvent.Load` and `.Unload` of the chunks of the line (`cx` is relative to the chunk of the origin).
        static void chunk(String kind, ChunkEvent<LevelChunk> event) {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished && event.getLevel() == recorder.level) {
                ChunkPos pos = event.getChunk().getPos();
                int cx = pos.x() - recorder.originChunk.x();
                if (cx >= 0 && cx <= CHUNKS && pos.z() == recorder.originChunk.z()) {
                    recorder.add(kind, "\"cx\":" + cx);
                }
            }
        }

        static void ticket(ChunkTicketLevelUpdatedEvent event) {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished && event.getLevel() == recorder.level) {
                ChunkPos pos = ChunkPos.unpack(event.getChunkPos());
                int cx = pos.x() - recorder.originChunk.x();
                if (cx >= 0 && cx <= CHUNKS && pos.z() == recorder.originChunk.z()) {
                    recorder.add("ticket", "\"cx\":" + cx + ",\"old\":" + event.getOldTicketLevel() + ",\"new\":" + event.getNewTicketLevel());
                }
            }
        }

        static void tile(BlockPos pos, int hash) {
            log("tile", pos, "\"hash\":" + hash);
        }

        static void log(String kind, BlockPos pos, String extra) {
            Recorder recorder = current;
            if (recorder == null || recorder.finished) {
                before(kind, pos, extra);
            }
            if (recorder != null && !recorder.finished) {
                int x = recorder.index(pos);
                //Only the cables of this structure
                if (x >= 0) {
                    recorder.add(kind, "\"x\":" + x + (extra.isEmpty() ? "" : "," + extra));
                }
            }
        }

        @Nullable
        private TileEntityUniversalCable cable(int x) {
            return WorldUtils.getTileEntity(TileEntityUniversalCable.class, level, origin.offset(x % width, 0, x / width), false);
        }

        /// The index of the cable at `pos` (`z * width + x`), or -1 when the position is not in the structure.
        private int index(BlockPos pos) {
            int x = pos.getX() - origin.getX();
            int z = pos.getZ() - origin.getZ();
            return x >= 0 && x < width && z >= 0 && z < depth && pos.getY() == origin.getY() ? z * width + x : -1;
        }

        private long relativeTick() {
            return ServerLifecycleHooks.getCurrentServer().getTickCount() - startTick;
        }

        /// The registry after `onTick`: every cable, every network that has a cable of the line, and the order of the registry set.
        private void recordState() {
            Map<Integer, TracedNetwork> nets = new TreeMap<>();
            StringBuilder cables = new StringBuilder();
            for (int x = 0; x < width * depth; x++) {
                cables.append(x == 0 ? "" : ",");
                TileEntityUniversalCable tile = cable(x);
                if (tile == null) {
                    cables.append("null");
                    continue;
                }
                UniversalCable cable = tile.getTransmitter();
                EnergyNetwork network = cable.getTransmitterNetwork();
                int index = -1;
                if (network instanceof TracedNetwork traced) {
                    index = traced.index;
                    nets.put(index, traced);
                }
                cables.append("{\"net\":").append(network == null ? "null" : Integer.toString(index))
                      .append(",\"orphan\":").append(cable.isOrphan())
                      .append(",\"valid\":").append(cable.isValid())
                      .append(",\"loaded\":").append(tile.isLoaded()).append('}');
            }
            StringBuilder registry = new StringBuilder();
            for (DynamicNetwork<?, ?, ?> network : registryNetworks()) {
                if (network instanceof TracedNetwork traced && inLine(traced)) {
                    nets.put(traced.index, traced);
                    registry.append(registry.isEmpty() ? "" : ",").append(traced.index);
                }
            }
            StringBuilder networkList = new StringBuilder();
            for (Map.Entry<Integer, TracedNetwork> entry : nets.entrySet()) {
                TracedNetwork network = entry.getValue();
                StringBuilder members = new StringBuilder();
                for (UniversalCable member : network.getTransmitters()) {
                    members.append(members.isEmpty() ? "" : ",").append(index(member.getBlockPos()));
                }
                networkList.append(networkList.isEmpty() ? "" : ",").append("{\"net\":").append(entry.getKey())
                      .append(",\"hash\":").append(network.getUUID().hashCode())
                      .append(",\"size\":").append(network.transmittersSize())
                      .append(",\"acceptors\":").append(network.getAcceptorCount())
                      .append(",\"transmitters\":[").append(members).append("]}");
            }
            add("state", "\"cables\":[" + cables + "],\"nets\":[" + networkList + "],\"registry\":[" + registry + "]");
            if (name.equals("acceptors")) {
                //Which sides of each cable the acceptor cache of the transmitter sees as acceptor now, and its connection bits
                StringBuilder diag = new StringBuilder();
                for (int x = 0; x < width * depth; x++) {
                    TileEntityUniversalCable tile = cable(x);
                    if (tile != null) {
                        int connected = 0;
                        for (Direction side : Direction.values()) {
                            if (tile.getTransmitter().getAcceptorCache().getConnectedAcceptor(side) != null) {
                                connected |= 1 << side.ordinal();
                            }
                        }
                        diag.append(diag.isEmpty() ? "" : ",").append("[").append(connected).append(",")
                              .append(tile.getTransmitter().getAcceptorCache().currentAcceptorConnections).append("]");
                    }
                }
                //The energy capability each side of the blocks put next to the line gives
                StringBuilder blocks = new StringBuilder();
                for (int x : new int[]{1, 3}) {
                    int mask = 0;
                    for (Direction side : Direction.values()) {
                        if (level.getCapability(Capabilities.ENERGY.block(), origin.offset(x, 0, 1), side) != null) {
                            mask |= 1 << side.ordinal();
                        }
                    }
                    blocks.append(blocks.isEmpty() ? "" : ",").append(mask);
                }
                add("diag", "\"cables\":[" + diag + "],\"blocks\":[" + blocks + "]");
            }
            flush();
        }

        private boolean inLine(TracedNetwork network) {
            for (UniversalCable member : network.getTransmitters()) {
                if (index(member.getBlockPos()) >= 0) {
                    return true;
                }
            }
            return false;
        }

        @SuppressWarnings("unchecked")
        private static Set<DynamicNetwork<?, ?, ?>> registryNetworks() {
            try {
                Field field = TransmitterNetworkRegistry.class.getDeclaredField("networks");
                field.setAccessible(true);
                return (Set<DynamicNetwork<?, ?, ?>>) field.get(null);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private void add(String kind, String fields) {
            lines.add("{\"t\":" + relativeTick() + ",\"e\":\"" + kind + "\"" + (fields.isEmpty() ? "" : "," + fields) + "}");
        }

        private void flush() {
            try {
                Path file = Path.of("trace", "network_" + name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }

        private record Scheduled(int tick, String mark, Runnable action) {
        }
    }

    /// The `ChunkData` of the Mekanism test, without the debug logging, on the chunk of the line (see [#lineChunk]).
    private static class ChunkData {

        private final int index;
        @Nullable
        private ChunkPos absolutePos;
        private long absPos;
        private boolean isLoaded = true;

        ChunkData(int index) {
            this.index = index;
        }

        int updateChunkLoading(MekGameTestHelper helper, boolean load, int level) {
            if (absolutePos == null) {
                absolutePos = lineChunk(helper, index);
                absPos = absolutePos.pack();
            }
            if (WorldUtils.isChunkLoaded(helper.getLevel(), absolutePos) != load) {
                ChunkMap chunkMap = helper.getChunkMap();
                DistanceManager distanceManager = chunkMap.getDistanceManager();
                ChunkHolder holder = distanceManager.getChunk(absPos);
                if ((holder == null) == load) {
                    isLoaded = !load;
                    if (load) {
                        holder = distanceManager.updateChunkScheduling(absPos, level, holder, MekGameTestHelper.UNLOAD_LEVEL);
                        if (holder == null) {
                            helper.fail("Error loading chunk");
                        } else {
                            distanceManager.chunksToUpdateFutures.add(holder);
                        }
                    } else {
                        level = holder.getTicketLevel();
                        distanceManager.updateChunkScheduling(absPos, MekGameTestHelper.UNLOAD_LEVEL, holder, level);
                        chunkMap.processUnloads(ConstantPredicates.ALWAYS_TRUE);
                    }
                }
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
                helper.fail("Chunk has not been marked as " + (loaded ? "loaded" : "unloaded") + " yet");
            }
        }
    }
}
