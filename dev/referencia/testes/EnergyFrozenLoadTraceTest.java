package mekanism.common.tests.network;

import java.io.IOException;
import mekanism.api.RelativeSide;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.tile.interfaces.ISideConfiguration;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.storage.TagValueOutput;
import mekanism.api.AutomationType;
import net.neoforged.neoforge.transfer.transaction.Transaction;
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

/// Grava a rede de energia (frente `rede-energia`, Fase 3), um `.jsonl` por cenário em `trace/energy_<cenário>.jsonl`, para
/// `mods/machinery_basis/core/tests/energy_network_trace.rs` repetir. O formato é o de `NetworkTopologyTraceTest` (os eventos `before_*`, `setup`,
/// `tile`, `mark`, `created`, `update`, `commit`, `invalidate`, `net_update`, `access` e o `state` da topologia) mais:
///
/// - `estate`: depois de `TransmitterNetworkRegistry.onTick`, a energia de cada cabo (`buf`, o `buffer` dele; `share`, o `saveShare`; `cap`, a
///   capacidade do tier), de cada rede (`energy`, o `energyContainer`; `capacity`; `scale` e `scaleBits`, o `currentScale`; `prev`, o
///   `prevTransferAmount`) e de cada ator posto (`actors`: a energia da máquina ou do cubo);
/// - `push`: a energia que um vizinho empurra pela capability do cabo (`amount`, e `got`, o que o cabo aceitou), no `ServerTickEvent.Pre`;
/// - `contents`: o `onContentsChanged` do contêiner da rede, com a energia nova (o cubo criativo empurra pelo ejetor dele, e o que entra antes do
///   `net_update` do tick é o que ele empurrou);
/// - `transfer`: o `EnergyTransferEvent` (que o jogo manda ao cliente como `PacketNetworkScale`), com o `currentScale`;
/// - `nbt`: a energia que `UniversalCable.write` grava (que antes valida as shares).
///
/// As marcas do cenário são `place x z tier`, `break x z`, `chamber nome x y z max guardada`, `source nome x y z`, `push x z quantidade`,
/// `setenergy nome energia`, `nbt x`, `unload C` e `load C`. Uma máquina posta por `setBlock` tem todos os lados `NONE` e não expõe a capability
/// de energia (o que a `rede-registro` viu): o conserto é dar-lhe a configuração de lados do item (`applyComponentsFromItemStack`) e `INPUT` nos seis
/// lados, como o rastro do cubo de energia. Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o
/// `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace_energy_frozen")
public class EnergyFrozenLoadTraceTest {

    private static final String EMPTY14 = MekanismTests.MODID + ":energy_frozen_empty14";
    private static final String STRAIGHT_48 = MekanismTests.MODID + ":energy_frozen_straight_48";
    private static final int WIDTH = 14;
    private static final int DEPTH = 4;
    private static final int CHUNKS = 3;
    private static final int TIMEOUT = 30 * SharedConstants.TICKS_PER_SECOND;
    private static final int SETUP = SharedConstants.TICKS_PER_SECOND;
    /// A machine capacity that no scenario fills.
    private static final long BIG = 1_000_000_000L;

    //Note: lazy so the universal cables are registered, and so the traced block entity is in place before the structure is placed
    @RegisterStructureTemplate(EMPTY14)
    public static final Supplier<StructureTemplate> EMPTY14_TEMPLATE = StructureTemplateBuilder.lazy(WIDTH, 2, DEPTH, builder -> {
        Recorder.installTracedCable();
        return builder.fill(WIDTH - 1, 0, DEPTH - 1, WIDTH - 1, 0, DEPTH - 1, Blocks.AIR.defaultBlockState());
    });

    @RegisterStructureTemplate(STRAIGHT_48)
    public static final Supplier<StructureTemplate> STRAIGHT_48_TEMPLATE = StructureTemplateBuilder.lazy(CHUNKS * 16, 1, 1, builder -> {
        Recorder.installTracedCable();
        return builder.fill(0, 0, 0, CHUNKS * 16 - 1, 0, 0, Blocks.AIR.defaultBlockState());
    });

    private static Block basic() {
        return MekanismBlocks.BASIC_UNIVERSAL_CABLE.get();
    }

    /// The server frozen before a chunk with transmitters unloads and loads again, and still frozen until a few ticks after the chunk is back: the block entities read from the
    /// disk (`clearRemoved` only sets `markJoined`) join the registry in their first `onUpdateServer`, which a frozen level does not run, so the cables do not join a network
    /// until the server runs again (the reopened world of the game, frozen from the first tick). Records `energy_frozen_load.jsonl`.
    @GameTest(template = STRAIGHT_48, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_energy_frozen_load")
    @TestHolder(description = "Records the energy while a chunk with energy is unloaded and loaded again with the server frozen, and after it runs again.")
    public static void frozenLoad(final DynamicTest test) {
        final ChunkData chunkData = new ChunkData(1);
        final java.util.concurrent.atomic.AtomicInteger unloaded = new java.util.concurrent.atomic.AtomicInteger();
        test.eventListeners().forge().addListener((final ChunkEvent.Unload event) -> chunkData.updateChunk(event, false));
        test.eventListeners().forge().addListener((final ChunkEvent.Load event) -> chunkData.updateChunk(event, true));
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("frozen_load", helper, CHUNKS * 16, 1);
                  for (int x = 0; x < CHUNKS * 16; x++) {
                      placeNow(helper, basic(), x, 0);
                  }
                  Recorder.schedule(6, "pre push 0 0 150000", () -> push(helper, 0, 0, 150000));
                  Recorder.schedule(7, "pre push 0 0 150000", () -> push(helper, 0, 0, 150000));
                  Recorder.schedule(12, "pre freeze", () -> ServerLifecycleHooks.getCurrentServer().tickRateManager().setFrozen(true));
                  Recorder.schedule(14, "pre unload 1", () -> unloaded.set(chunkData.updateChunkLoading(helper, false, MekGameTestHelper.UNLOAD_LEVEL)));
                  Recorder.schedule(20, "pre load 1", () -> chunkData.updateChunkLoading(helper, true, unloaded.get()));
                  Recorder.schedule(40, "pre unfreeze", () -> ServerLifecycleHooks.getCurrentServer().tickRateManager().setFrozen(false));
                  Recorder.schedule(60, "pre finish", Recorder::finish);
              })
              .thenWaitUntil(() -> {
                  if (!Recorder.isFinished()) {
                      helper.fail("Trace not finished yet");
                  }
              })
              .thenSucceed()
        );
    }








    /// The chunk that holds the cables around the relative x `16 * index + 8`. The chunk of `MekGameTestHelper.absolutePos(ChunkPos)` takes the
    /// middle block at z = 8, which is another chunk of z when the structure sits at z % 16 >= 8, and then the line is not in it.
    private static ChunkPos lineChunk(MekGameTestHelper helper, int index) {
        return ChunkPos.containing(helper.absolutePos(new BlockPos(16 * index + 8, 0, 0)));
    }

    /// The relative x of the first block of the chunk `index` of the line (see [#lineChunk]).
    private static int chunkStart(MekGameTestHelper helper, int index) {
        return lineChunk(helper, index).getMinBlockX() - helper.absolutePos(BlockPos.ZERO).getX();
    }



    /// `MekGameTestHelper.setChunkLoadLevel` for an absolute chunk.
    private static int setChunkLoadLevel(MekGameTestHelper helper, ChunkPos chunk, int newLevel) {
        DistanceManager distanceManager = helper.getChunkMap().getDistanceManager();
        ChunkHolder holder = distanceManager.getChunk(chunk.pack());
        int oldLevel = holder == null ? MekGameTestHelper.UNLOAD_LEVEL : holder.getTicketLevel();
        distanceManager.updateChunkScheduling(chunk.pack(), newLevel, holder, oldLevel);
        return oldLevel;
    }

    /// Places a cable at the relative `x`, `z` (mark `place x z`, with the block name for the tier).
    private static void place(MekGameTestHelper helper, Block block, int x, int z) {
        helper.setBlock(x, 0, z, block.defaultBlockState());
    }

    private static void placeNow(MekGameTestHelper helper, Block block, int x, int z) {
        Recorder.mark("place " + x + " " + z + " " + tierOf(block));
        place(helper, block, x, z);
    }

    private static void chamberNow(MekGameTestHelper helper, String name, int x, int y, int z, long max, long stored) {
        Recorder.mark("chamber " + name + " " + x + " " + y + " " + z + " " + max + " " + stored);
        chamber(helper, name, x, y, z, max, stored);
    }

    private static void waitFinished(MekGameTestHelper helper) {
        if (!Recorder.isFinished()) {
            helper.fail("Trace not finished yet");
        }
    }

    private static String tierOf(Block block) {
        return block == MekanismBlocks.BASIC_UNIVERSAL_CABLE.get() ? "basic"
               : block == MekanismBlocks.ADVANCED_UNIVERSAL_CABLE.get() ? "advanced"
               : block == MekanismBlocks.ELITE_UNIVERSAL_CABLE.get() ? "elite" : "ultimate";
    }

    private static void breakAt(MekGameTestHelper helper, int x, int z) {
        helper.setBlock(x, 0, z, Blocks.AIR);
    }

    /// An Enrichment Chamber at the relative x, y(0 or 1), z, which accepts energy on every side (a block put by `setBlock` has every side `NONE`,
    /// so it exposes no energy capability: the side config of an item is what makes a placed machine accept energy), with the given capacity and
    /// stored energy.
    private static void chamber(MekGameTestHelper helper, String name, int x, int y, int z, long max, long stored) {
        helper.setBlock(x, y, z, MekanismBlocks.ENRICHMENT_CHAMBER.defaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, Direction.SOUTH));
        TileEntityMekanism tile = WorldUtils.getTileEntity(TileEntityMekanism.class, helper.getLevel(), helper.absolutePos(new BlockPos(x, y, z)));
        //Which sides expose the energy capability: as put by setBlock, after the item components, and after INPUT on every side
        int placed = capabilityMask(helper, x, y, z);
        tile.applyComponentsFromItemStack(new ItemStack(MekanismBlocks.ENRICHMENT_CHAMBER.asItem()));
        int components = capabilityMask(helper, x, y, z);
        ConfigInfo info = ((ISideConfiguration) tile).getConfig().getConfig(TransmissionType.ENERGY);
        for (RelativeSide relative : RelativeSide.VALUES) {
            info.setDataType(DataType.INPUT, relative);
        }
        Recorder.event("capdiag", "\"name\":\"" + name + "\",\"placed\":" + placed + ",\"components\":" + components + ",\"input\":" + capabilityMask(helper, x, y, z));
        ((MachineEnergyContainer<?>) tile.getEnergyContainer()).setMaxEnergy(max);
        tile.getEnergyContainer().setEnergy(stored, null);
        Recorder.actor(name, new BlockPos(x, y, z));
    }

    /// A creative energy cube at the relative x, z whose sides all output energy, the source that pushes at its transfer rate.
    private static void source(MekGameTestHelper helper, String name, int x, int y, int z) {
        helper.setBlock(x, y, z + 1, MekanismBlocks.CREATIVE_ENERGY_CUBE.defaultState().setValue(BlockStateProperties.FACING, Direction.UP));
        TileEntityMekanism tile = WorldUtils.getTileEntity(TileEntityMekanism.class, helper.getLevel(), helper.absolutePos(new BlockPos(x, y, z + 1)));
        tile.applyComponentsFromItemStack(new ItemStack(MekanismBlocks.CREATIVE_ENERGY_CUBE.asItem()));
        ConfigInfo info = ((ISideConfiguration) tile).getConfig().getConfig(TransmissionType.ENERGY);
        for (RelativeSide relative : RelativeSide.VALUES) {
            info.setDataType(DataType.OUTPUT, relative);
        }
        info.setEjecting(true);
        tile.getEnergyContainer().setEnergy(Long.MAX_VALUE, null);
        Recorder.actor(name, new BlockPos(x, y, z + 1));
    }

    private static int capabilityMask(MekGameTestHelper helper, int x, int y, int z) {
        int mask = 0;
        for (Direction side : Direction.values()) {
            if (helper.getLevel().getCapability(Capabilities.ENERGY.block(), helper.absolutePos(new BlockPos(x, y, z)), side) != null) {
                mask |= 1 << side.ordinal();
            }
        }
        return mask;
    }

    private static void setEnergy(MekGameTestHelper helper, String name, long energy) {
        TileEntityMekanism tile = Recorder.actorTile(name);
        tile.getEnergyContainer().setEnergy(energy, null);
    }

    /// Pushes energy into the cable at the relative x, z the way a neighbour does (`insert` of the container the capability hands out, with the EXTERNAL
    /// automation), and logs what it took. The capability itself refuses a side that is not connected, which the scenarios do not set up.
    private static void push(MekGameTestHelper helper, int x, int z, int amount) {
        TileEntityUniversalCable tile = WorldUtils.getTileEntity(TileEntityUniversalCable.class, helper.getLevel(), helper.absolutePos(new BlockPos(x, 0, z)));
        int got = 0;
        if (tile != null) {
            try (Transaction transaction = Transaction.openRoot()) {
                got = tile.getTransmitter().getContainer().insert(amount, transaction, AutomationType.EXTERNAL);
                transaction.commit();
            }
        }
        Recorder.event("push", "\"x\":" + x + ",\"z\":" + z + ",\"amount\":" + amount + ",\"got\":" + got);
    }

    private static void listen(DynamicTest test) {
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> Recorder.pre());
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> Recorder.post());
        test.eventListeners().forge().addListener((final ChunkEvent.Load event) -> Recorder.chunk("load", event));
        test.eventListeners().forge().addListener((final ChunkEvent.Unload event) -> Recorder.chunk("unload", event));
        test.eventListeners().forge().addListener((final ChunkTicketLevelUpdatedEvent event) -> Recorder.ticket(event));
        test.eventListeners().forge().addListener((final EnergyNetwork.EnergyTransferEvent event) -> Recorder.transfer(event));
    }

    /// The cable block entity, with a transmitter that logs `requestsUpdate` and creates the traced network.
    private static class TracedCable extends TileEntityUniversalCable {

        TracedCable(Holder<Block> blockProvider, BlockPos pos, BlockState state) {
            super(blockProvider, pos, state);
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
        public void onContentsChanged() {
            Recorder.event("contents", "\"net\":" + index + ",\"energy\":" + energyContainer.getAmountAsLong());
            super.onContentsChanged();
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

        /// Puts [TracedCable] in place of the factory of the four cable block entity types, as a no-op while nothing is recording.
        static synchronized void installTracedCable() {
            if (installed) {
                return;
            }
            try {
                Field factory = BlockEntityType.class.getDeclaredField("factory");
                factory.setAccessible(true);
                install(factory, MekanismTileEntityTypes.BASIC_UNIVERSAL_CABLE.get(), MekanismBlocks.BASIC_UNIVERSAL_CABLE);
                install(factory, MekanismTileEntityTypes.ADVANCED_UNIVERSAL_CABLE.get(), MekanismBlocks.ADVANCED_UNIVERSAL_CABLE);
                install(factory, MekanismTileEntityTypes.ELITE_UNIVERSAL_CABLE.get(), MekanismBlocks.ELITE_UNIVERSAL_CABLE);
                install(factory, MekanismTileEntityTypes.ULTIMATE_UNIVERSAL_CABLE.get(), MekanismBlocks.ULTIMATE_UNIVERSAL_CABLE);
                installed = true;
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private static void install(Field factory, BlockEntityType<TileEntityUniversalCable> type, Holder<Block> provider) throws IllegalAccessException {
            BlockEntityType.BlockEntitySupplier<TileEntityUniversalCable> traced = (pos, state) -> new TracedCable(provider, pos, state);
            factory.set(type, traced);
        }

        private final java.util.Map<String, BlockPos> actors = new java.util.LinkedHashMap<>();

        static void actor(String name, BlockPos relative) {
            Recorder recorder = current;
            if (recorder != null) {
                recorder.actors.put(name, relative);
            }
        }

        @Nullable
        static TileEntityMekanism actorTile(String name) {
            Recorder recorder = current;
            BlockPos relative = recorder == null ? null : recorder.actors.get(name);
            return relative == null ? null : WorldUtils.getTileEntity(TileEntityMekanism.class, recorder.level, recorder.origin.offset(relative.getX(), relative.getY(), relative.getZ()));
        }

        static long relativeTickNow() {
            Recorder recorder = current;
            return recorder == null ? 0 : recorder.relativeTick();
        }

        static void transfer(EnergyNetwork.EnergyTransferEvent event) {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished && event.network instanceof TracedNetwork traced) {
                recorder.add("transfer", "\"net\":" + traced.index + ",\"scale\":" + event.network.currentScale);
            }
        }

        /// What `UniversalCable.write` saves (it validates the save shares first, once per game tick).
        static void nbt(int x) {
            Recorder recorder = current;
            if (recorder == null || recorder.finished) {
                return;
            }
            TileEntityUniversalCable tile = recorder.cable(x);
            if (tile == null) {
                recorder.add("nbt", "\"x\":" + x + ",\"energy\":-1");
                return;
            }
            TagValueOutput out = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, recorder.level.registryAccess());
            tile.getTransmitter().write(out);
            CompoundTag tag = out.buildResult();
            recorder.add("nbt", "\"x\":" + x + ",\"energy\":" + tag.getLongOr("energy", 0L));
        }

        private static long saveShare(UniversalCable cable) {
            try {
                Field journal = UniversalCable.class.getDeclaredField("saveShareJournal");
                journal.setAccessible(true);
                Object value = journal.get(cable);
                Field share = value.getClass().getDeclaredField("saveShare");
                share.setAccessible(true);
                return share.getLong(value);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private static long prevTransfer(EnergyNetwork network) {
            try {
                Field field = EnergyNetwork.class.getDeclaredField("prevTransferAmount");
                field.setAccessible(true);
                return field.getLong(network);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        /// The energy of every cable, network and actor after `onTick`.
        private void recordEnergy(Map<Integer, TracedNetwork> nets) {
            StringBuilder cables = new StringBuilder();
            for (int x = 0; x < width * depth; x++) {
                TileEntityUniversalCable tile = cable(x);
                if (tile == null) {
                    continue;
                }
                UniversalCable cable = tile.getTransmitter();
                cables.append(cables.isEmpty() ? "" : ",").append("{\"x\":").append(x)
                      .append(",\"buf\":").append(cable.buffer.getAmountAsLong())
                      .append(",\"share\":").append(saveShare(cable))
                      .append(",\"cap\":").append(cable.getCapacity()).append('}');
            }
            StringBuilder networks = new StringBuilder();
            for (Map.Entry<Integer, TracedNetwork> entry : nets.entrySet()) {
                TracedNetwork network = entry.getValue();
                networks.append(networks.isEmpty() ? "" : ",").append("{\"net\":").append(entry.getKey())
                      .append(",\"energy\":").append(network.energyContainer.getAmountAsLong())
                      .append(",\"capacity\":").append(network.getCapacity())
                      .append(",\"scale\":").append(network.currentScale)
                      .append(",\"scaleBits\":").append(Float.floatToRawIntBits(network.currentScale))
                      .append(",\"prev\":").append(prevTransfer(network)).append('}');
            }
            StringBuilder actorList = new StringBuilder();
            for (Map.Entry<String, BlockPos> entry : actors.entrySet()) {
                BlockPos relative = entry.getValue();
                TileEntityMekanism tile = WorldUtils.getTileEntity(TileEntityMekanism.class, level, origin.offset(relative.getX(), relative.getY(), relative.getZ()));
                actorList.append(actorList.isEmpty() ? "" : ",").append("{\"name\":\"").append(entry.getKey()).append("\",\"energy\":")
                      .append(tile == null ? -1 : tile.getEnergyContainer().getAmountAsLong()).append('}');
            }
            add("estate", "\"cables\":[" + cables + "],\"nets\":[" + networks + "],\"actors\":[" + actorList + "]");
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
            } else if (!kind.equals("net_update") && !kind.equals("contents")) {
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
            recordEnergy(nets);
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
                Path file = Path.of("trace", "energy_" + name + ".jsonl");
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
