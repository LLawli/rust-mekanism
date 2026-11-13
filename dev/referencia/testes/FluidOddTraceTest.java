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
import mekanism.api.SerializationConstants;
import mekanism.api.fluid.IFluidTank;
import mekanism.api.functions.ConstantPredicates;
import mekanism.api.resource.LargeResourceStack;
import mekanism.common.capabilities.Capabilities;
import mekanism.common.content.network.FluidNetwork;
import mekanism.common.content.network.transmitter.BufferedResourceTransmitter;
import mekanism.common.content.network.transmitter.MechanicalPipe;
import mekanism.common.lib.transmitter.ConnectionType;
import mekanism.common.lib.transmitter.DynamicNetwork;
import mekanism.common.lib.transmitter.TransmitterNetworkRegistry;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismTileEntityTypes;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.TileEntityFluidTank;
import mekanism.common.tile.transmitter.TileEntityMechanicalPipe;
import mekanism.common.util.WorldUtils;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ChunkHolder;
import net.minecraft.server.level.ChunkMap;
import net.minecraft.server.level.DistanceManager;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.structure.templatesystem.StructureTemplate;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.world.level.storage.ValueInput;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.event.level.ChunkTicketLevelUpdatedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import net.neoforged.neoforge.transfer.ResourceHandler;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.RegisterStructureTemplate;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;
import org.jspecify.annotations.Nullable;

/// Grava a rede de fluido (frente `rede-fluido`, Fase 3), um `.jsonl` por cenário em `trace/fluid_<cenário>.jsonl`, para o teste Rust repetir. O
/// formato é o de `EnergyNetworkTraceTest` (os eventos `before_*`, `setup`, `tile`, `mark`, `created`, `update`, `commit`, `invalidate`,
/// `net_update`, `access` e o `state` da topologia, que continua chamando os canos de `cables`) mais:
///
/// - `estate`: depois de `TransmitterNetworkRegistry.onTick`, o fluido de cada cano (`pipes`: `buf` e `fluid`, o `bufferContainer`; `share` e
///   `shareFluid`, o `saveShare`; `cap`, a capacidade do tier), de cada rede (`nets`: `fluid` e `amount`, o contêiner; `capacity`; `scale` e
///   `scaleBits`, o `currentScale`; `prev`, o `prevTransferAmount`; `last`, o `getLastType()`) e de cada ator posto (`actors`: `fluid` e `amount` do
///   tanque, ou do `lavaTank` do Heat Generator);
/// - `push`: o fluido que um vizinho empurra pela capability de fluido do cano (`Capabilities.FLUID.block()`, no primeiro lado que expõe o contêiner,
///   `side`), com `insert` numa transação raiz confirmada (`fluid`, `amount`, e `got`, o que entrou), no `ServerTickEvent.Pre`;
/// - `contents`: o `onContentsChanged` do contêiner da rede, com o que ele guarda agora (`fluid`, `amount`);
/// - `transfer`: o `FluidTransferEvent` (que o jogo manda ao cliente como `PacketNetworkScale`), com o `currentScale` e o `fluid` do evento;
/// - `nbt`: o que `MechanicalPipe.write` grava em `stored` (que antes valida as shares), `fluid` e `amount`;
/// - `capdiag` (mask dos lados que expõem a capability de fluido do tanque ou do gerador posto) e `modediag` (as conexões que o cano tem e teria, e se
///   ele não tem fluido para ser a rede primária, lidos antes de `onModeChange`): diagnóstico, sem efeito no jogo;
/// - `adopt`: o `adoptTransmittersAndAcceptorsFrom` que a fusão manual (`recheckConnectionPrechecked`) chama, gravado antes do `super` (`net` é a
///   rede que adota e `from` a adotada). Os que o construtor `FluidNetwork(Collection)` chama já aparecem em `created.from` e não são gravados.
///
/// As marcas do cenário são `place x z tier`, `break x z`, `tank nome x y z tier fluido quantidade` (`fluido` é `empty` quando vazio),
/// `source nome x y z fluido` (um tanque criativo cheio do fluido), `generator nome x y z` (o Heat Generator), `push x z fluido quantidade`,
/// `setfluid nome fluido quantidade` (`quantidade` é `max` para a capacidade), `pull x z lado`, `mode x z lado tipo` (o que a chave inglesa faz:
/// `setConnectionTypeRaw`, `onModeChange`, `refreshConnections` e `notifyTileChange`), `nbt x`, `unload C` e `load C`. Um bloco posto por `setBlock`
/// tem os lados do tanque que o item dá; o `capdiag` diz quais lados expõem a capability de fluido. Copie o arquivo para
/// `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace_fluid_odd")
public class FluidOddTraceTest {

    private static final String EMPTY14 = MekanismTests.MODID + ":fluid_odd_empty14";
    private static final String STRAIGHT_48 = MekanismTests.MODID + ":fluid_odd_straight_48";
    private static final int WIDTH = 14;
    private static final int DEPTH = 4;
    private static final int CHUNKS = 3;
    private static final int TIMEOUT = 40 * SharedConstants.TICKS_PER_SECOND;
    private static final int SETUP = SharedConstants.TICKS_PER_SECOND;
    private static final String WATER = "minecraft:water";
    private static final String LAVA = "minecraft:lava";

    //Note: lazy so the mechanical pipes are registered, and so the traced block entity is in place before the structure is placed
    @RegisterStructureTemplate(EMPTY14)
    public static final Supplier<StructureTemplate> EMPTY14_TEMPLATE = StructureTemplateBuilder.lazy(WIDTH, 2, DEPTH, builder -> {
        Recorder.installTracedPipe();
        return builder.fill(WIDTH - 1, 0, DEPTH - 1, WIDTH - 1, 0, DEPTH - 1, Blocks.AIR.defaultBlockState());
    });

    @RegisterStructureTemplate(STRAIGHT_48)
    public static final Supplier<StructureTemplate> STRAIGHT_48_TEMPLATE = StructureTemplateBuilder.lazy(CHUNKS * 16, 1, 1, builder -> {
        Recorder.installTracedPipe();
        return builder.fill(0, 0, 0, CHUNKS * 16 - 1, 0, 0, Blocks.AIR.defaultBlockState());
    });

    private static Block basic() {
        return MekanismBlocks.BASIC_MECHANICAL_PIPE.get();
    }

    private static Block advanced() {
        return MekanismBlocks.ADVANCED_MECHANICAL_PIPE.get();
    }

    private static Block elite() {
        return MekanismBlocks.ELITE_MECHANICAL_PIPE.get();
    }

    private static Block ultimate() {
        return MekanismBlocks.ULTIMATE_MECHANICAL_PIPE.get();
    }

    /// A scheduled action: the mark is `pre ` plus the text, and runs in the `ServerTickEvent.Pre` of the relative tick.
    private static void at(int tick, String text, Runnable action) {
        Recorder.schedule(tick, "pre " + text, action);
    }

    private static void atPush(MekGameTestHelper helper, int tick, int x, int z, String fluid, int amount) {
        at(tick, "push " + x + " " + z + " " + fluid + " " + amount, () -> push(helper, x, z, fluid, amount));
    }

    private static void atTank(MekGameTestHelper helper, int tick, String name, int x, int y, int z, String tier, String fluid, long amount) {
        at(tick, "tank " + name + " " + x + " " + y + " " + z + " " + tier + " " + fluid + " " + amount, () -> tank(helper, name, x, y, z, tier, fluid, amount));
    }

    private static void atSource(MekGameTestHelper helper, int tick, String name, int x, int y, int z, String fluid) {
        at(tick, "source " + name + " " + x + " " + y + " " + z + " " + fluid, () -> tank(helper, name, x, y, z, "creative", fluid, -1));
    }

    private static void atSetFluid(MekGameTestHelper helper, int tick, String name, String fluid, long amount) {
        at(tick, "setfluid " + name + " " + fluid + " " + (amount < 0 ? "max" : Long.toString(amount)), () -> setFluid(name, fluid, amount));
    }

    private static void atPull(MekGameTestHelper helper, int tick, int x, int z, Direction side) {
        at(tick, "pull " + x + " " + z + " " + side.getName(), () -> mode(helper, x, z, side, ConnectionType.PULL));
    }

    private static void atMode(MekGameTestHelper helper, int tick, int x, int z, Direction side, ConnectionType type) {
        at(tick, "mode " + x + " " + z + " " + side.getName() + " " + type.getSerializedName(), () -> mode(helper, x, z, side, type));
    }

    private static void atPlace(MekGameTestHelper helper, int tick, Block block, int x, int z) {
        at(tick, "place " + x + " " + z + " " + tierOf(block), () -> place(helper, block, x, z));
    }

    private static void atBreak(MekGameTestHelper helper, int tick, int x, int z) {
        at(tick, "break " + x + " " + z, () -> breakAt(helper, x, z));
    }










    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_fluid_odd_0")
    @TestHolder(description = "Records a network with an odd amount of fluid losing its end pipes one after the other.")
    public static void breakEndOdd0(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("odd_0", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, x == 1 ? advanced() : basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, WATER, 1001);
                  atBreak(helper, 8, 4, 0);
                  atBreak(helper, 14, 0, 0);
                  atBreak(helper, 20, 2, 0);
                  at(28, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_fluid_odd_1")
    @TestHolder(description = "Records a network with an odd amount of fluid losing its end pipes one after the other.")
    public static void breakEndOdd1(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("odd_1", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, x == 1 ? advanced() : basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, WATER, 1001);
                  atBreak(helper, 8, 4, 0);
                  atBreak(helper, 14, 0, 0);
                  atBreak(helper, 20, 2, 0);
                  at(28, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_fluid_odd_2")
    @TestHolder(description = "Records a network with an odd amount of fluid losing its end pipes one after the other.")
    public static void breakEndOdd2(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("odd_2", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, x == 1 ? advanced() : basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, WATER, 1001);
                  atBreak(helper, 8, 4, 0);
                  atBreak(helper, 14, 0, 0);
                  atBreak(helper, 20, 2, 0);
                  at(28, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_fluid_odd_3")
    @TestHolder(description = "Records a network with an odd amount of fluid losing its end pipes one after the other.")
    public static void breakEndOdd3(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("odd_3", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, x == 1 ? advanced() : basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, WATER, 1001);
                  atBreak(helper, 8, 4, 0);
                  atBreak(helper, 14, 0, 0);
                  atBreak(helper, 20, 2, 0);
                  at(28, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_fluid_odd_4")
    @TestHolder(description = "Records a network with an odd amount of fluid losing its end pipes one after the other.")
    public static void breakEndOdd4(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("odd_4", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, x == 1 ? advanced() : basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, WATER, 1001);
                  atBreak(helper, 8, 4, 0);
                  atBreak(helper, 14, 0, 0);
                  atBreak(helper, 20, 2, 0);
                  at(28, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_fluid_odd_5")
    @TestHolder(description = "Records a network with an odd amount of fluid losing its end pipes one after the other.")
    public static void breakEndOdd5(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("odd_5", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, x == 1 ? advanced() : basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, WATER, 1001);
                  atBreak(helper, 8, 4, 0);
                  atBreak(helper, 14, 0, 0);
                  atBreak(helper, 20, 2, 0);
                  at(28, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_fluid_odd_6")
    @TestHolder(description = "Records a network with an odd amount of fluid losing its end pipes one after the other.")
    public static void breakEndOdd6(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("odd_6", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, x == 1 ? advanced() : basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, WATER, 1001);
                  atBreak(helper, 8, 4, 0);
                  atBreak(helper, 14, 0, 0);
                  atBreak(helper, 20, 2, 0);
                  at(28, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_fluid_odd_7")
    @TestHolder(description = "Records a network with an odd amount of fluid losing its end pipes one after the other.")
    public static void breakEndOdd7(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("odd_7", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, x == 1 ? advanced() : basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, WATER, 1001);
                  atBreak(helper, 8, 4, 0);
                  atBreak(helper, 14, 0, 0);
                  atBreak(helper, 20, 2, 0);
                  at(28, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_fluid_odd_8")
    @TestHolder(description = "Records a network with an odd amount of fluid losing its end pipes one after the other.")
    public static void breakEndOdd8(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("odd_8", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, x == 1 ? advanced() : basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, WATER, 1001);
                  atBreak(helper, 8, 4, 0);
                  atBreak(helper, 14, 0, 0);
                  atBreak(helper, 20, 2, 0);
                  at(28, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_fluid_odd_9")
    @TestHolder(description = "Records a network with an odd amount of fluid losing its end pipes one after the other.")
    public static void breakEndOdd9(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("odd_9", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, x == 1 ? advanced() : basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, WATER, 1001);
                  atBreak(helper, 8, 4, 0);
                  atBreak(helper, 14, 0, 0);
                  atBreak(helper, 20, 2, 0);
                  at(28, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }



    /// The chunk that holds the pipes around the relative x `16 * index + 8`. The chunk of `MekGameTestHelper.absolutePos(ChunkPos)` takes the
    /// middle block at z = 8, which is another chunk of z when the structure sits at z % 16 >= 8, and then the line is not in it.
    private static ChunkPos lineChunk(MekGameTestHelper helper, int index) {
        return ChunkPos.containing(helper.absolutePos(new BlockPos(16 * index + 8, 0, 0)));
    }

    /// The relative x of the first block of the chunk `index` of the line (see [#lineChunk]).
    private static int chunkStart(MekGameTestHelper helper, int index) {
        return lineChunk(helper, index).getMinBlockX() - helper.absolutePos(BlockPos.ZERO).getX();
    }



    /// Places a pipe at the relative x, z (mark `place x z`, with the block name for the tier).
    private static void place(MekGameTestHelper helper, Block block, int x, int z) {
        helper.setBlock(x, 0, z, block.defaultBlockState());
    }

    private static void placeNow(MekGameTestHelper helper, Block block, int x, int z) {
        Recorder.mark("place " + x + " " + z + " " + tierOf(block));
        place(helper, block, x, z);
    }

    private static void tankNow(MekGameTestHelper helper, String name, int x, int y, int z, String tier, String fluid, long amount) {
        Recorder.mark("tank " + name + " " + x + " " + y + " " + z + " " + tier + " " + fluid + " " + amount);
        tank(helper, name, x, y, z, tier, fluid, amount);
    }

    private static void generatorNow(MekGameTestHelper helper, String name, int x, int y, int z) {
        Recorder.mark("generator " + name + " " + x + " " + y + " " + z);
        helper.setBlock(x, y, z, GeneratorsBlocks.HEAT_GENERATOR.defaultState());
        Recorder.event("capdiag", "\"name\":\"" + name + "\",\"mask\":" + capabilityMask(helper, x, y, z));
        Recorder.actor(name, new BlockPos(x, y, z));
    }

    private static void modeNow(MekGameTestHelper helper, int x, int z, Direction side, ConnectionType type) {
        Recorder.mark("mode " + x + " " + z + " " + side.getName() + " " + type.getSerializedName());
        mode(helper, x, z, side, type);
    }

    private static void waitFinished(MekGameTestHelper helper) {
        if (!Recorder.isFinished()) {
            helper.fail("Trace not finished yet");
        }
    }

    private static String tierOf(Block block) {
        return block == MekanismBlocks.BASIC_MECHANICAL_PIPE.get() ? "basic"
               : block == MekanismBlocks.ADVANCED_MECHANICAL_PIPE.get() ? "advanced"
               : block == MekanismBlocks.ELITE_MECHANICAL_PIPE.get() ? "elite" : "ultimate";
    }

    private static void breakAt(MekGameTestHelper helper, int x, int z) {
        helper.setBlock(x, 0, z, Blocks.AIR);
    }

    private static Fluid fluidOf(String fluid) {
        return switch (fluid) {
            case WATER -> Fluids.WATER;
            case LAVA -> Fluids.LAVA;
            default -> throw new IllegalArgumentException(fluid);
        };
    }

    private static BlockState tankState(String tier) {
        return switch (tier) {
            case "basic" -> MekanismBlocks.BASIC_FLUID_TANK.defaultState();
            case "advanced" -> MekanismBlocks.ADVANCED_FLUID_TANK.defaultState();
            case "elite" -> MekanismBlocks.ELITE_FLUID_TANK.defaultState();
            case "ultimate" -> MekanismBlocks.ULTIMATE_FLUID_TANK.defaultState();
            case "creative" -> MekanismBlocks.CREATIVE_FLUID_TANK.defaultState();
            default -> throw new IllegalArgumentException(tier);
        };
    }

    /// A Fluid Tank of the tier at the relative x, y(0 or 1), z with the given fluid and amount (`-1` is the capacity, which is what a creative tank
    /// holds). A creative tank that holds a fluid gives an infinite supply to a pipe that pulls from it.
    private static void tank(MekGameTestHelper helper, String name, int x, int y, int z, String tier, String fluid, long amount) {
        helper.setBlock(x, y, z, tankState(tier));
        TileEntityFluidTank tile = WorldUtils.getTileEntity(TileEntityFluidTank.class, helper.getLevel(), helper.absolutePos(new BlockPos(x, y, z)));
        if (!fluid.equals("empty") && amount != 0) {
            FluidResource resource = FluidResource.of(fluidOf(fluid));
            tile.fluidTank.setContents(resource, amount < 0 ? tile.fluidTank.capacityAsLong(resource) : amount, null);
        }
        Recorder.event("capdiag", "\"name\":\"" + name + "\",\"mask\":" + capabilityMask(helper, x, y, z));
        Recorder.actor(name, new BlockPos(x, y, z));
    }

    private static int capabilityMask(MekGameTestHelper helper, int x, int y, int z) {
        int mask = 0;
        for (Direction side : Direction.values()) {
            if (helper.getLevel().getCapability(Capabilities.FLUID.block(), helper.absolutePos(new BlockPos(x, y, z)), side) != null) {
                mask |= 1 << side.ordinal();
            }
        }
        return mask;
    }

    private static void setFluid(String name, String fluid, long amount) {
        IFluidTank tank = Recorder.actorTank(name);
        if (tank == null) {
            return;
        }
        if (fluid.equals("empty") || amount == 0) {
            tank.setContents(LargeResourceStack.FLUID_HELPER.empty(), null);
        } else {
            FluidResource resource = FluidResource.of(fluidOf(fluid));
            tank.setContents(resource, amount < 0 ? tank.capacityAsLong(resource) : amount, null);
        }
    }

    /// What the wrench does to a side: the raw type, `onModeChange`, `refreshConnections` and `notifyTileChange`. A `PULL` side makes the pipe pull from
    /// the acceptor there (`pullFromAcceptors`); `NORMAL` on a side that already is `NORMAL` just rechecks the connections.
    private static void mode(MekGameTestHelper helper, int x, int z, Direction side, ConnectionType type) {
        TileEntityMechanicalPipe tile = WorldUtils.getTileEntity(TileEntityMechanicalPipe.class, helper.getLevel(), helper.absolutePos(new BlockPos(x, 0, z)));
        if (tile != null) {
            MechanicalPipe pipe = tile.getTransmitter();
            pipe.setConnectionTypeRaw(side, type);
            //What the recheck will see: the connections now and the ones the pipe would have, and whether it has any fluid to be the primary network
            Recorder.event("modediag", "\"x\":" + x + ",\"z\":" + z + ",\"current\":" + pipe.currentTransmitterConnections + ",\"possible\":"
                                     + pipe.getPossibleTransmitterConnections() + ",\"noBuffer\":" + pipe.noBufferOrFallback());
            pipe.onModeChange(side);
            pipe.refreshConnections();
            pipe.notifyTileChange();
        }
    }

    /// Pushes fluid into the pipe at the relative x, z the way a neighbour does (`insert` of the handler the capability hands out on the first side that
    /// exposes a container, in a root transaction that is committed), and logs what it took.
    private static void push(MekGameTestHelper helper, int x, int z, String fluid, int amount) {
        BlockPos pos = helper.absolutePos(new BlockPos(x, 0, z));
        FluidResource resource = FluidResource.of(fluidOf(fluid));
        int got = 0;
        String sideName = "none";
        TileEntityMechanicalPipe tile = WorldUtils.getTileEntity(TileEntityMechanicalPipe.class, helper.getLevel(), pos);
        for (Direction side : Direction.values()) {
            //The first side the pipe accepts on (a side with no connection hands out a handler that takes nothing)
            if (tile == null || !tile.getTransmitter().getConnectionType(side).canAccept()) {
                continue;
            }
            ResourceHandler<FluidResource> handler = helper.getLevel().getCapability(Capabilities.FLUID.block(), pos, side);
            if (handler != null) {
                try (Transaction transaction = Transaction.openRoot()) {
                    got = handler.insert(resource, amount, transaction);
                    transaction.commit();
                }
                sideName = side.getName();
                break;
            }
        }
        Recorder.event("push", "\"x\":" + x + ",\"z\":" + z + ",\"fluid\":\"" + fluid + "\",\"amount\":" + amount + ",\"got\":" + got + ",\"side\":\"" + sideName + "\"");
    }

    private static void listen(DynamicTest test) {
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> Recorder.pre());
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> Recorder.post());
        test.eventListeners().forge().addListener((final ChunkEvent.Load event) -> Recorder.chunk("load", event));
        test.eventListeners().forge().addListener((final ChunkEvent.Unload event) -> Recorder.chunk("unload", event));
        test.eventListeners().forge().addListener((final ChunkTicketLevelUpdatedEvent event) -> Recorder.ticket(event));
        test.eventListeners().forge().addListener((final FluidNetwork.FluidTransferEvent event) -> Recorder.transfer(event));
    }

    private static String id(FluidResource resource) {
        return resource.isEmpty() ? "" : resource.typeHolder().getRegisteredName();
    }

    /// The pipe block entity, with a transmitter that logs `requestsUpdate` and creates the traced network.
    private static class TracedPipe extends TileEntityMechanicalPipe {

        TracedPipe(Holder<Block> blockProvider, BlockPos pos, BlockState state) {
            super(blockProvider, pos, state);
            Recorder.tile(pos, System.identityHashCode(getTransmitter()));
        }

        @Override
        protected MechanicalPipe createTransmitter(Holder<Block> blockProvider) {
            return new TracedTransmitter(blockProvider, this);
        }

        @Override
        public void chunkAccessibilityChange(boolean loaded) {
            Recorder.log("access", getBlockPos(), "\"loaded\":" + loaded);
            super.chunkAccessibilityChange(loaded);
        }
    }

    private static class TracedTransmitter extends MechanicalPipe {

        TracedTransmitter(Holder<Block> blockProvider, TracedPipe tile) {
            super(blockProvider, tile);
        }

        @Override
        public FluidNetwork createNetworkByMerging(Collection<FluidNetwork> networks) {
            return new TracedNetwork(networks);
        }

        @Override
        public void requestsUpdate() {
            Recorder.log("update", getBlockPos(), "");
            super.requestsUpdate();
        }
    }

    private static class TracedNetwork extends FluidNetwork {

        private final int index;
        /// False while the constructor of [FluidNetwork] runs (it adopts the merged networks and fills the container before `index` is known).
        private boolean ready;

        TracedNetwork(Collection<FluidNetwork> from) {
            super(from);
            index = Recorder.nextNetworkIndex();
            ready = true;
            StringBuilder merged = new StringBuilder();
            for (FluidNetwork network : from) {
                merged.append(merged.isEmpty() ? "" : ",").append(network instanceof TracedNetwork traced ? traced.index : -1);
            }
            Recorder.event("created", "\"net\":" + index + ",\"hash\":" + getUUID().hashCode() + ",\"from\":[" + merged + "]");
        }

        /// The index of this network, which the constructor of [FluidNetwork] cannot read yet: it is the next one to be handed out.
        private int idx() {
            return ready ? index : Recorder.peekNetworkIndex();
        }

        @Override
        public List<MechanicalPipe> adoptTransmittersAndAcceptorsFrom(FluidNetwork net) {
            if (ready) {
                Recorder.event("adopt", "\"net\":" + index + ",\"from\":" + (net instanceof TracedNetwork traced ? traced.index : -1));
            }
            return super.adoptTransmittersAndAcceptorsFrom(net);
        }

        @Override
        public void onUpdate() {
            Recorder.event("net_update", "\"net\":" + index);
            super.onUpdate();
        }

        @Override
        public void onContentsChanged() {
            if (container != null) {
                Recorder.event("contents", "\"net\":" + idx() + ",\"fluid\":\"" + id(container.resource()) + "\",\"amount\":" + container.amountAsLong());
            }
            super.onContentsChanged();
        }

        @Override
        public void commit() {
            Recorder.event("commit", "\"net\":" + index);
            super.commit();
        }

        @Override
        public void invalidate(@Nullable MechanicalPipe triggerTransmitter) {
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

        /// Puts [TracedPipe] in place of the factory of the four pipe block entity types, as a no-op while nothing is recording.
        static synchronized void installTracedPipe() {
            if (installed) {
                return;
            }
            try {
                Field factory = BlockEntityType.class.getDeclaredField("factory");
                factory.setAccessible(true);
                install(factory, MekanismTileEntityTypes.BASIC_MECHANICAL_PIPE.get(), MekanismBlocks.BASIC_MECHANICAL_PIPE);
                install(factory, MekanismTileEntityTypes.ADVANCED_MECHANICAL_PIPE.get(), MekanismBlocks.ADVANCED_MECHANICAL_PIPE);
                install(factory, MekanismTileEntityTypes.ELITE_MECHANICAL_PIPE.get(), MekanismBlocks.ELITE_MECHANICAL_PIPE);
                install(factory, MekanismTileEntityTypes.ULTIMATE_MECHANICAL_PIPE.get(), MekanismBlocks.ULTIMATE_MECHANICAL_PIPE);
                installed = true;
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private static void install(Field factory, BlockEntityType<TileEntityMechanicalPipe> type, Holder<Block> provider) throws IllegalAccessException {
            BlockEntityType.BlockEntitySupplier<TileEntityMechanicalPipe> traced = (pos, state) -> new TracedPipe(provider, pos, state);
            factory.set(type, traced);
        }

        private final java.util.Map<String, BlockPos> actors = new java.util.LinkedHashMap<>();

        static void actor(String name, BlockPos relative) {
            Recorder recorder = current;
            if (recorder != null) {
                recorder.actors.put(name, relative);
            }
        }

        /// The fluid tank of an actor: the tank of a Fluid Tank or the `lavaTank` of a Heat Generator.
        @Nullable
        static IFluidTank actorTank(String name) {
            Recorder recorder = current;
            BlockPos relative = recorder == null ? null : recorder.actors.get(name);
            return relative == null ? null : recorder.tankAt(relative);
        }

        @Nullable
        private IFluidTank tankAt(BlockPos relative) {
            BlockPos pos = origin.offset(relative.getX(), relative.getY(), relative.getZ());
            if (level.getBlockEntity(pos) instanceof TileEntityFluidTank tank) {
                return tank.fluidTank;
            } else if (level.getBlockEntity(pos) instanceof TileEntityHeatGenerator generator) {
                return generator.lavaTank;
            }
            return null;
        }

        static long relativeTickNow() {
            Recorder recorder = current;
            return recorder == null ? 0 : recorder.relativeTick();
        }

        static void transfer(FluidNetwork.FluidTransferEvent event) {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished && event.network instanceof TracedNetwork traced) {
                recorder.add("transfer", "\"net\":" + traced.index + ",\"scale\":" + event.network.currentScale + ",\"fluid\":\"" + id(event.fluidType) + "\"");
            }
        }

        /// What `MechanicalPipe.write` saves (it validates the save shares first, once per game tick).
        static void nbt(int x) {
            Recorder recorder = current;
            if (recorder == null || recorder.finished) {
                return;
            }
            TileEntityMechanicalPipe tile = recorder.pipe(x);
            if (tile == null) {
                recorder.add("nbt", "\"x\":" + x + ",\"fluid\":\"\",\"amount\":-1");
                return;
            }
            TagValueOutput out = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, recorder.level.registryAccess());
            tile.getTransmitter().write(out);
            CompoundTag tag = out.buildResult();
            ValueInput input = TagValueInput.create(ProblemReporter.DISCARDING, recorder.level.registryAccess(), tag);
            LargeResourceStack<FluidResource> stored = LargeResourceStack.FLUID_HELPER.readOrEmpty(input, SerializationConstants.STORED);
            recorder.add("nbt", "\"x\":" + x + ",\"fluid\":\"" + id(stored.resource()) + "\",\"amount\":" + stored.amount());
        }

        @SuppressWarnings("unchecked")
        private static LargeResourceStack<FluidResource> saveShare(MechanicalPipe pipe) {
            try {
                Field journal = BufferedResourceTransmitter.class.getDeclaredField("saveShareJournal");
                journal.setAccessible(true);
                Object value = journal.get(pipe);
                Field share = value.getClass().getDeclaredField("saveShare");
                share.setAccessible(true);
                return (LargeResourceStack<FluidResource>) share.get(value);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        /// The fluid of every pipe, network and actor after `onTick`.
        private void recordFluid(Map<Integer, TracedNetwork> nets) {
            StringBuilder pipes = new StringBuilder();
            for (int x = 0; x < width * depth; x++) {
                TileEntityMechanicalPipe tile = pipe(x);
                if (tile == null) {
                    continue;
                }
                MechanicalPipe pipe = tile.getTransmitter();
                LargeResourceStack<FluidResource> buffer = pipe.getShare();
                LargeResourceStack<FluidResource> share = saveShare(pipe);
                pipes.append(pipes.isEmpty() ? "" : ",").append("{\"x\":").append(x)
                      .append(",\"buf\":").append(buffer.amount())
                      .append(",\"fluid\":\"").append(id(buffer.resource())).append('"')
                      .append(",\"share\":").append(share.amount())
                      .append(",\"shareFluid\":\"").append(id(share.resource())).append('"')
                      .append(",\"cap\":").append(pipe.getCapacity()).append('}');
            }
            StringBuilder networks = new StringBuilder();
            for (Map.Entry<Integer, TracedNetwork> entry : nets.entrySet()) {
                TracedNetwork network = entry.getValue();
                networks.append(networks.isEmpty() ? "" : ",").append("{\"net\":").append(entry.getKey())
                      .append(",\"fluid\":\"").append(id(network.getContainer().resource())).append('"')
                      .append(",\"amount\":").append(network.getContainer().amountAsLong())
                      .append(",\"capacity\":").append(network.getCapacity())
                      .append(",\"scale\":").append(network.currentScale)
                      .append(",\"scaleBits\":").append(Float.floatToRawIntBits(network.currentScale))
                      .append(",\"prev\":").append(network.getPrevTransferAmount())
                      .append(",\"last\":\"").append(id(network.getLastType())).append("\"}");
            }
            StringBuilder actorList = new StringBuilder();
            for (Map.Entry<String, BlockPos> entry : actors.entrySet()) {
                IFluidTank tank = tankAt(entry.getValue());
                actorList.append(actorList.isEmpty() ? "" : ",").append("{\"name\":\"").append(entry.getKey()).append("\",\"fluid\":\"")
                      .append(tank == null ? "" : id(tank.resource())).append("\",\"amount\":").append(tank == null ? -1 : tank.amountAsLong()).append('}');
            }
            add("estate", "\"pipes\":[" + pipes + "],\"nets\":[" + networks + "],\"actors\":[" + actorList + "]");
        }

        static synchronized int nextNetworkIndex() {
            return networks++;
        }

        static synchronized int peekNetworkIndex() {
            return networks;
        }

        /// What happened before `begin`, for the structure being built: tiles and networks, with the absolute position of the pipes.
        private record Before(long tick, String kind, @Nullable BlockPos pos, String fields) {
        }

        private static final List<Before> BEFORE = new ArrayList<>();

        static synchronized void before(String kind, @Nullable BlockPos pos, String fields) {
            BEFORE.add(new Before(ServerLifecycleHooks.getCurrentServer().getTickCount(), kind, pos, fields));
            if (BEFORE.size() > 20000) {
                BEFORE.subList(0, 5000).clear();
            }
        }

        /// A line (`depth` 1) or a plane of pipes; the pipe at `x`, `z` has the index `z * width + x`.
        static void begin(String name, MekGameTestHelper helper, int width, int depth) {
            Recorder recorder = new Recorder(name, helper, width, depth);
            StringBuilder tiles = new StringBuilder();
            for (int x = 0; x < width * depth; x++) {
                TileEntityMechanicalPipe tile = recorder.pipe(x);
                if (tile != null) {
                    tiles.append(tiles.isEmpty() ? "" : ",").append("{\"x\":").append(x).append(",\"hash\":")
                          .append(System.identityHashCode(tile.getTransmitter())).append('}');
                }
            }
            recorder.add("setup", "\"x\":" + recorder.origin.getX() + ",\"y\":" + recorder.origin.getY() + ",\"z\":" + recorder.origin.getZ()
                                  + ",\"width\":" + width + ",\"depth\":" + depth + ",\"xInChunk\":" + (recorder.origin.getX() & 15) + ",\"tiles\":[" + tiles + "]");
            current = recorder;
            //How the structure was built, as `before_*` events with the tick relative to `begin` (negative), pipes of the line and every network
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
            } else if (!kind.equals("net_update") && !kind.equals("contents") && !kind.equals("adopt") && !kind.equals("capdiag") && !kind.equals("push")) {
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
                //Only the pipes of this structure
                if (x >= 0) {
                    recorder.add(kind, "\"x\":" + x + (extra.isEmpty() ? "" : "," + extra));
                }
            }
        }

        @Nullable
        private TileEntityMechanicalPipe pipe(int x) {
            return WorldUtils.getTileEntity(TileEntityMechanicalPipe.class, level, origin.offset(x % width, 0, x / width), false);
        }

        /// The index of the pipe at `pos` (`z * width + x`), or -1 when the position is not in the structure.
        private int index(BlockPos pos) {
            int x = pos.getX() - origin.getX();
            int z = pos.getZ() - origin.getZ();
            return x >= 0 && x < width && z >= 0 && z < depth && pos.getY() == origin.getY() ? z * width + x : -1;
        }

        private long relativeTick() {
            return ServerLifecycleHooks.getCurrentServer().getTickCount() - startTick;
        }

        /// The registry after `onTick`: every pipe, every network that has a pipe of the line, and the order of the registry set.
        private void recordState() {
            Map<Integer, TracedNetwork> nets = new TreeMap<>();
            StringBuilder cables = new StringBuilder();
            for (int x = 0; x < width * depth; x++) {
                cables.append(x == 0 ? "" : ",");
                TileEntityMechanicalPipe tile = pipe(x);
                if (tile == null) {
                    cables.append("null");
                    continue;
                }
                MechanicalPipe pipe = tile.getTransmitter();
                FluidNetwork network = pipe.getTransmitterNetwork();
                int index = -1;
                if (network instanceof TracedNetwork traced) {
                    index = traced.index;
                    nets.put(index, traced);
                }
                cables.append("{\"net\":").append(network == null ? "null" : Integer.toString(index))
                      .append(",\"orphan\":").append(pipe.isOrphan())
                      .append(",\"valid\":").append(pipe.isValid())
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
                for (MechanicalPipe member : network.getTransmitters()) {
                    members.append(members.isEmpty() ? "" : ",").append(index(member.getBlockPos()));
                }
                networkList.append(networkList.isEmpty() ? "" : ",").append("{\"net\":").append(entry.getKey())
                      .append(",\"hash\":").append(network.getUUID().hashCode())
                      .append(",\"size\":").append(network.transmittersSize())
                      .append(",\"acceptors\":").append(network.getAcceptorCount())
                      .append(",\"transmitters\":[").append(members).append("]}");
            }
            add("state", "\"cables\":[" + cables + "],\"nets\":[" + networkList + "],\"registry\":[" + registry + "]");
            recordFluid(nets);
            flush();
        }

        private boolean inLine(TracedNetwork network) {
            for (MechanicalPipe member : network.getTransmitters()) {
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
                Path file = Path.of("trace", "fluid_" + name + ".jsonl");
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
