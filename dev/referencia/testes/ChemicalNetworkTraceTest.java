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
import mekanism.api.RelativeSide;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.chemical.IChemicalTank;
import mekanism.api.MekanismRegistries;
import mekanism.api.functions.ConstantPredicates;
import mekanism.api.resource.LargeResourceStack;
import mekanism.common.capabilities.Capabilities;
import mekanism.common.content.network.ChemicalNetwork;
import mekanism.common.content.network.transmitter.BufferedResourceTransmitter;
import mekanism.common.content.network.transmitter.PressurizedTube;
import mekanism.common.lib.transmitter.ConnectionType;
import mekanism.common.lib.transmitter.DynamicNetwork;
import mekanism.common.lib.transmitter.TransmitterNetworkRegistry;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismTileEntityTypes;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.tile.TileEntityChemicalTank;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.tile.interfaces.ISideConfiguration;
import mekanism.common.tile.transmitter.TileEntityPressurizedTube;
import mekanism.common.util.WorldUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.item.ItemStack;
import mekanism.api.chemical.Chemical;
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
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.world.level.storage.ValueInput;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.event.level.ChunkTicketLevelUpdatedEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import net.neoforged.neoforge.transfer.ResourceHandler;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.RegisterStructureTemplate;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;
import org.jspecify.annotations.Nullable;

/// Grava a rede de químico (frente `rede-quimico`, Fase 3), um `.jsonl` por cenário em `trace/chemical_<cenário>.jsonl`, para o teste Rust repetir. É o
/// `FluidNetworkTraceTest` com tubos pressurizados e tanques de químico (os eventos `before_*`, `setup`, `tile`, `mark`, `created`, `update`, `commit`,
/// `invalidate`, `net_update`, `access` e o `state` da topologia, que continua chamando os tubos de `cables`) mais:
///
/// - `estate`: depois de `TransmitterNetworkRegistry.onTick`, o químico de cada tubo (`tubes`: `buf` e `chem`, o `bufferContainer`; `share` e
///   `shareChem`, o `saveShare`; `cap`, a capacidade do tier), de cada rede (`nets`: `chem` e `amount`, o contêiner; `capacity`; `scale` e
///   `scaleBits`, o `currentScale`; `prev`, o `prevTransferAmount`; `last`, o `getLastType()`) e de cada ator posto (`actors`: `chem` e `amount` do tanque);
/// - `push`: o químico que um vizinho empurra pela capability de químico do tubo (`Capabilities.CHEMICAL.block()`, no primeiro lado que expõe o contêiner,
///   `side`), com `insert` numa transação raiz confirmada (`chem`, `amount`, e `got`, o que entrou), no `ServerTickEvent.Pre`;
/// - `contents`: o `onContentsChanged` do contêiner da rede, com o que ele guarda agora (`chem`, `amount`);
/// - `transfer`: o `ChemicalTransferEvent` (que o jogo manda ao cliente como `PacketNetworkScale`), com o `currentScale` e o `chem` do evento;
/// - `disperse`: o `ChemicalNetwork.disperse(trigger, resource, amount)` (o vazamento de radiação), com o tubo que o disparou (`x`), o `chem` e o `amount`;
///   gravado antes do `super`, que despeja a radiação no nível;
/// - `nbt`: o que `PressurizedTube.write` grava em `stored` (que antes valida as shares), `chem` e `amount`;
/// - `capdiag` (`placed`, `components` e `io`: a máscara dos lados que expõem a capability de químico do tanque posto por `setBlock`, depois da
///   configuração do item e depois de `INPUT_OUTPUT` nos seis lados; `ejecting`, o que o item dá) e `modediag` (as conexões que o tubo tem e teria, e se
///   ele não tem químico para ser a rede primária, lidos antes de `onModeChange`): diagnóstico, sem efeito no jogo;
/// - `adopt`: o `adoptTransmittersAndAcceptorsFrom` que a fusão manual (`recheckConnectionPrechecked`) chama, gravado antes do `super` (`net` é a
///   rede que adota e `from` a adotada). Os que o construtor `ChemicalNetwork(Collection)` chama já aparecem em `created.from` e não são gravados.
///
/// As marcas do cenário são `place x z tier`, `break x z`, `tank nome x y z tier químico quantidade` (`químico` é `empty` quando vazio),
/// `source nome x y z químico` (um tanque criativo cheio do químico), `push x z químico quantidade`, `setchem nome químico quantidade` (`quantidade` é
/// `max` para a capacidade), `pull x z lado`, `mode x z lado tipo` (o que a chave inglesa faz: `setConnectionTypeRaw`, `onModeChange`,
/// `refreshConnections` e `notifyTileChange`), `nbt x`, `unload C` e `load C`. O Chemical Tank é uma máquina configurável: posto por `setBlock` tem todos os
/// lados `NONE` e não expõe a capability (decisão 113), então recebe a configuração do item (`applyComponentsFromItemStack`) e `INPUT_OUTPUT` nos seis
/// lados, sem ejetar. Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace_chemical")
public class ChemicalNetworkTraceTest {

    private static final String EMPTY14 = MekanismTests.MODID + ":chemical_trace_empty14";
    private static final String STRAIGHT_48 = MekanismTests.MODID + ":chemical_trace_straight_48";
    private static final int WIDTH = 14;
    private static final int DEPTH = 4;
    private static final int CHUNKS = 3;
    private static final int TIMEOUT = 40 * SharedConstants.TICKS_PER_SECOND;
    private static final int SETUP = SharedConstants.TICKS_PER_SECOND;
    private static final String HYDROGEN = "mekanism:hydrogen";
    private static final String OXYGEN = "mekanism:oxygen";
    private static final String WASTE = "mekanism:nuclear_waste";

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
        return MekanismBlocks.BASIC_PRESSURIZED_TUBE.get();
    }

    private static Block advanced() {
        return MekanismBlocks.ADVANCED_PRESSURIZED_TUBE.get();
    }

    private static Block elite() {
        return MekanismBlocks.ELITE_PRESSURIZED_TUBE.get();
    }

    private static Block ultimate() {
        return MekanismBlocks.ULTIMATE_PRESSURIZED_TUBE.get();
    }

    /// A scheduled action: the mark is `pre ` plus the text, and runs in the `ServerTickEvent.Pre` of the relative tick.
    private static void at(int tick, String text, Runnable action) {
        Recorder.schedule(tick, "pre " + text, action);
    }

    private static void atPush(MekGameTestHelper helper, int tick, int x, int z, String chemical, int amount) {
        at(tick, "push " + x + " " + z + " " + chemical + " " + amount, () -> push(helper, x, z, chemical, amount));
    }

    private static void atTank(MekGameTestHelper helper, int tick, String name, int x, int y, int z, String tier, String chemical, long amount) {
        at(tick, "tank " + name + " " + x + " " + y + " " + z + " " + tier + " " + chemical + " " + amount, () -> tank(helper, name, x, y, z, tier, chemical, amount));
    }

    private static void atSource(MekGameTestHelper helper, int tick, String name, int x, int y, int z, String chemical) {
        at(tick, "source " + name + " " + x + " " + y + " " + z + " " + chemical, () -> tank(helper, name, x, y, z, "creative", chemical, -1));
    }

    private static void atSetChem(MekGameTestHelper helper, int tick, String name, String chemical, long amount) {
        at(tick, "setchem " + name + " " + chemical + " " + (amount < 0 ? "max" : Long.toString(amount)), () -> setChem(name, chemical, amount));
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

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_line_consumer")
    @TestHolder(description = "Records a source pushing and then pulling into a line of basic tubes with a tank at the side.")
    public static void lineConsumer(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("line_consumer", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, HYDROGEN, 3000);
                  atPush(helper, 5, 0, 0, HYDROGEN, 3000);
                  atPush(helper, 6, 0, 0, HYDROGEN, 100000);
                  atTank(helper, 10, "A", 4, 0, 1, "basic", "empty", 0);
                  atSource(helper, 16, "S", 0, 0, 1, HYDROGEN);
                  atPull(helper, 16, 0, 0, Direction.SOUTH);
                  atSetChem(helper, 30, "S", OXYGEN, -1);
                  atTank(helper, 44, "B", 3, 0, 1, "basic", "empty", 0);
                  at(56, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_two_consumers")
    @TestHolder(description = "Records three tanks that ask for different amounts, so the split leaves a remainder, and one that holds another chemical.")
    public static void twoConsumers(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("two_consumers", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, basic(), x, 0);
                  }
                  tankNow(helper, "A", 1, 0, 1, "basic", HYDROGEN, 63950);
                  tankNow(helper, "B", 3, 0, 1, "advanced", HYDROGEN, 255000);
                  tankNow(helper, "C", 2, 1, 0, "elite", HYDROGEN, 1023900);
                  atPush(helper, 4, 0, 0, HYDROGEN, 100);
                  atSetChem(helper, 8, "A", HYDROGEN, 63995);
                  atSetChem(helper, 8, "B", HYDROGEN, 255990);
                  atSetChem(helper, 8, "C", HYDROGEN, 1023995);
                  atPush(helper, 9, 0, 0, HYDROGEN, 30);
                  atSetChem(helper, 12, "A", "empty", 0);
                  atSetChem(helper, 12, "B", "empty", 0);
                  atSetChem(helper, 12, "C", "empty", 0);
                  atPush(helper, 13, 0, 0, HYDROGEN, 7);
                  atPush(helper, 14, 0, 0, HYDROGEN, 1000);
                  atPush(helper, 15, 0, 0, HYDROGEN, 2);
                  atPush(helper, 16, 0, 0, HYDROGEN, 3);
                  atSetChem(helper, 20, "B", OXYGEN, 500);
                  atPush(helper, 21, 0, 0, HYDROGEN, 301);
                  atPush(helper, 22, 0, 0, HYDROGEN, 3000);
                  at(30, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_tiers")
    @TestHolder(description = "Records tubes of the four tiers in one network: the capacity, the clamp, the scale and the pull rate, with a tube broken and a tank drinking.")
    public static void tiers(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("tiers", helper, WIDTH, DEPTH);
                  placeNow(helper, basic(), 0, 0);
                  placeNow(helper, advanced(), 1, 0);
                  placeNow(helper, elite(), 2, 0);
                  placeNow(helper, ultimate(), 3, 0);
                  placeNow(helper, basic(), 4, 0);
                  atPush(helper, 4, 0, 0, HYDROGEN, 30000);
                  atPush(helper, 5, 0, 0, HYDROGEN, 30000);
                  atPush(helper, 6, 0, 0, HYDROGEN, 30000);
                  atPush(helper, 7, 0, 0, HYDROGEN, 30000);
                  atPush(helper, 8, 0, 0, HYDROGEN, 60000);
                  atBreak(helper, 14, 2, 0);
                  atTank(helper, 18, "X", 0, 0, 1, "basic", "empty", 0);
                  atTank(helper, 18, "A", 4, 0, 1, "basic", "empty", 0);
                  atTank(helper, 18, "Y", 4, 1, 0, "basic", "empty", 0);
                  atSource(helper, 24, "S", 1, 0, 1, HYDROGEN);
                  atPull(helper, 24, 1, 0, Direction.SOUTH);
                  atSource(helper, 28, "T", 3, 0, 1, HYDROGEN);
                  atPull(helper, 28, 3, 0, Direction.SOUTH);
                  atPlace(helper, 36, elite(), 2, 0);
                  at(48, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    /// Hydrogen network (x 0..2) and oxygen network (x 4..6) with a gap between them. The bridge tube at x 3 is an orphan and empty, and it joins one of
    /// them. The oxygen network is drained by a tank, and the tube at x 4 rechecks its west side (the wrench does that), which merges by hand.
    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_incompatible")
    @TestHolder(description = "Records a hydrogen network and a oxygen network joined by a bridge tube, the oxygen one drained, and the side rechecked.")
    public static void incompatible(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("incompatible", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 3; x++) {
                      placeNow(helper, basic(), x, 0);
                      placeNow(helper, basic(), x + 4, 0);
                  }
                  atPush(helper, 4, 0, 0, HYDROGEN, 10000);
                  atPush(helper, 4, 4, 0, OXYGEN, 10000);
                  atPlace(helper, 8, basic(), 3, 0);
                  atMode(helper, 12, 4, 0, Direction.WEST, ConnectionType.NORMAL);
                  atMode(helper, 14, 2, 0, Direction.EAST, ConnectionType.NORMAL);
                  atTank(helper, 16, "C", 6, 0, 1, "basic", "empty", 0);
                  atMode(helper, 17, 4, 0, Direction.WEST, ConnectionType.NORMAL);
                  atMode(helper, 18, 4, 0, Direction.WEST, ConnectionType.NORMAL);
                  atMode(helper, 19, 4, 0, Direction.WEST, ConnectionType.NORMAL);
                  atMode(helper, 24, 4, 0, Direction.WEST, ConnectionType.NORMAL);
                  atMode(helper, 25, 2, 0, Direction.EAST, ConnectionType.NORMAL);
                  atTank(helper, 32, "D", 2, 0, 1, "basic", "empty", 0);
                  at(42, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    /// The same, but the hydrogen network is the drained one: the tube that rechecks has the oxygen network, which still holds chemical.
    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_incompatible_swap")
    @TestHolder(description = "Records a bridge between a hydrogen and a oxygen network, with the hydrogen one drained and the oxygen tube rechecking.")
    public static void incompatibleSwap(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("incompatible_swap", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 3; x++) {
                      placeNow(helper, basic(), x, 0);
                      placeNow(helper, basic(), x + 4, 0);
                  }
                  atPush(helper, 4, 0, 0, HYDROGEN, 10000);
                  atPush(helper, 4, 4, 0, OXYGEN, 10000);
                  atPlace(helper, 8, basic(), 3, 0);
                  atTank(helper, 16, "C", 0, 0, 1, "basic", "empty", 0);
                  atMode(helper, 17, 4, 0, Direction.WEST, ConnectionType.NORMAL);
                  atMode(helper, 18, 4, 0, Direction.WEST, ConnectionType.NORMAL);
                  atMode(helper, 24, 4, 0, Direction.WEST, ConnectionType.NORMAL);
                  atMode(helper, 25, 2, 0, Direction.EAST, ConnectionType.NORMAL);
                  atTank(helper, 32, "D", 6, 0, 1, "basic", "empty", 0);
                  at(42, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    /// Hydrogen (x 0..2) and oxygen (x 3..5) tubes side by side with no bridge: they stay apart because the buffers differ. Both networks are drained and the
    /// tube at x 3 rechecks its west side.
    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_incompatible_direct")
    @TestHolder(description = "Records a hydrogen network and a oxygen network side by side, both drained, and the side rechecked.")
    public static void incompatibleDirect(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("incompatible_direct", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 6; x++) {
                      placeNow(helper, x < 3 ? basic() : advanced(), x, 0);
                  }
                  modeNow(helper, 2, 0, Direction.EAST, ConnectionType.NONE);
                  modeNow(helper, 3, 0, Direction.WEST, ConnectionType.NONE);
                  atPush(helper, 4, 0, 0, HYDROGEN, 10000);
                  atPush(helper, 4, 5, 0, OXYGEN, 18000);
                  atMode(helper, 10, 2, 0, Direction.EAST, ConnectionType.NORMAL);
                  atMode(helper, 11, 3, 0, Direction.WEST, ConnectionType.NORMAL);
                  atTank(helper, 14, "C", 0, 0, 1, "basic", "empty", 0);
                  atTank(helper, 14, "D", 5, 0, 1, "basic", "empty", 0);
                  atMode(helper, 15, 3, 0, Direction.WEST, ConnectionType.NORMAL);
                  atMode(helper, 16, 3, 0, Direction.WEST, ConnectionType.NORMAL);
                  atMode(helper, 22, 3, 0, Direction.WEST, ConnectionType.NORMAL);
                  atMode(helper, 23, 2, 0, Direction.EAST, ConnectionType.NORMAL);
                  atPush(helper, 28, 0, 0, HYDROGEN, 800);
                  at(38, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    /// Two networks of the same chemical kept apart by `NONE` on the two sides that face each other. Enabling the sides again rechecks the connection and
    /// merges the two full networks by hand.
    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_incompatible_none")
    @TestHolder(description = "Records two full hydrogen networks apart by NONE sides, joined again by the wrench path.")
    public static void incompatibleNone(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("incompatible_none", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 3; x++) {
                      placeNow(helper, basic(), x, 0);
                      placeNow(helper, advanced(), x + 3, 0);
                  }
                  modeNow(helper, 2, 0, Direction.EAST, ConnectionType.NONE);
                  modeNow(helper, 3, 0, Direction.WEST, ConnectionType.NONE);
                  atPush(helper, 4, 0, 0, HYDROGEN, 10000);
                  atPush(helper, 4, 5, 0, HYDROGEN, 40000);
                  atMode(helper, 12, 2, 0, Direction.EAST, ConnectionType.NORMAL);
                  atMode(helper, 14, 3, 0, Direction.WEST, ConnectionType.NORMAL);
                  atTank(helper, 22, "A", 5, 0, 1, "basic", "empty", 0);
                  at(32, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_merge_same")
    @TestHolder(description = "Records two full networks of the same chemical that a tube joins, and then a tank that drinks the merged network.")
    public static void mergeSame(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("merge_same", helper, WIDTH, DEPTH);
                  placeNow(helper, basic(), 0, 0);
                  placeNow(helper, basic(), 1, 0);
                  placeNow(helper, advanced(), 3, 0);
                  placeNow(helper, basic(), 4, 0);
                  atPush(helper, 4, 0, 0, HYDROGEN, 4000);
                  atPush(helper, 4, 4, 0, HYDROGEN, 18000);
                  atPlace(helper, 10, basic(), 2, 0);
                  atPush(helper, 16, 0, 0, HYDROGEN, 10000);
                  atTank(helper, 20, "A", 2, 0, 1, "basic", "empty", 0);
                  at(34, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_split_middle")
    @TestHolder(description = "Records the middle tube of a network with chemical being broken (the shares), put back, and a tank next to a broken tube.")
    public static void splitMiddle(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("split_middle", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, HYDROGEN, 12000);
                  atBreak(helper, 10, 2, 0);
                  atPlace(helper, 18, basic(), 2, 0);
                  atTank(helper, 26, "A", 4, 1, 0, "basic", HYDROGEN, 63000);
                  atPush(helper, 27, 0, 0, HYDROGEN, 8000);
                  atBreak(helper, 30, 3, 0);
                  at(40, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_break_end_odd")
    @TestHolder(description = "Records a network with an odd amount of chemical losing its end tubes one after the other.")
    public static void breakEndOdd(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("break_end_odd", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 5; x++) {
                      placeNow(helper, x == 1 ? advanced() : basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, HYDROGEN, 3001);
                  atBreak(helper, 8, 4, 0);
                  atBreak(helper, 14, 0, 0);
                  atBreak(helper, 20, 2, 0);
                  at(28, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    /// The last tube of a network being broken with a radioactive chemical inside: `disperse` is called (`onLastTransmitterRemoved`) and dumps radiation. A
    /// network of three tubes is split by the middle one, and the two halves lose their only tube one after the other.
    @GameTest(template = EMPTY14, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_radioactive")
    @TestHolder(description = "Records a network with a radioactive chemical losing its tubes until the last one, and the disperse calls that follow.")
    public static void radioactive(final DynamicTest test) {
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("radioactive", helper, WIDTH, DEPTH);
                  for (int x = 0; x < 3; x++) {
                      placeNow(helper, basic(), x, 0);
                  }
                  atPush(helper, 4, 0, 0, WASTE, 6001);
                  atBreak(helper, 10, 1, 0);
                  atBreak(helper, 16, 0, 0);
                  atBreak(helper, 22, 2, 0);
                  at(30, "finish", Recorder::finish);
              })
              .thenWaitUntil(() -> waitFinished(helper))
              .thenSucceed()
        );
    }

    /// The chunk that holds the tubes around the relative x `16 * index + 8`. The chunk of `MekGameTestHelper.absolutePos(ChunkPos)` takes the
    /// middle block at z = 8, which is another chunk of z when the structure sits at z % 16 >= 8, and then the line is not in it.
    private static ChunkPos lineChunk(MekGameTestHelper helper, int index) {
        return ChunkPos.containing(helper.absolutePos(new BlockPos(16 * index + 8, 0, 0)));
    }

    /// The relative x of the first block of the chunk `index` of the line (see [#lineChunk]).
    private static int chunkStart(MekGameTestHelper helper, int index) {
        return lineChunk(helper, index).getMinBlockX() - helper.absolutePos(BlockPos.ZERO).getX();
    }

    /// 144000 over 48 tubes is 3000 each: every share is equal, so the order in which the chunk hands the tubes back (the order of the hash table of its block
    /// entities, which depends on what the chunk held before) does not show in the amounts. With a remainder (160000), one run of the original gave an
    /// order the port could not know.
    @GameTest(template = STRAIGHT_48, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_chunk_reload")
    @TestHolder(description = "Records the chemical while the middle chunk of a line with chemical is unloaded and loaded again.")
    public static void chunkReload(final DynamicTest test) {
        final ChunkData chunkData = new ChunkData(1);
        test.eventListeners().forge().addListener((final ChunkEvent.Unload event) -> chunkData.updateChunk(event, false));
        test.eventListeners().forge().addListener((final ChunkEvent.Load event) -> chunkData.updateChunk(event, true));
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("chunk_reload", helper, CHUNKS * 16, 1);
                  for (int x = 0; x < CHUNKS * 16; x++) {
                      placeNow(helper, basic(), x, 0);
                  }
                  atPush(helper, 6, 0, 0, HYDROGEN, 100000);
                  atPush(helper, 7, 0, 0, HYDROGEN, 44000);
                  at(10, "nbt 0", () -> Recorder.nbt(0));
                  at(10, "nbt 30", () -> Recorder.nbt(30));
                  at(10, "nbt 47", () -> Recorder.nbt(47));
              })
              .thenWaitUntil(() -> {
                  if (Recorder.relativeTickNow() < 14) {
                      helper.fail("Waiting for the chemical");
                  }
              })
              .thenMap(() -> {
                  Recorder.mark("unload 1");
                  return chunkData.updateChunkLoading(helper, false, MekGameTestHelper.UNLOAD_LEVEL);
              })
              .thenWaitUntil(() -> chunkData.waitFor(helper, false))
              .thenExecuteAfter(5, level -> {
                  Recorder.mark("nbt 0");
                  Recorder.nbt(0);
                  Recorder.mark("load 1");
                  chunkData.updateChunkLoading(helper, true, level);
              })
              .thenWaitUntil(() -> chunkData.waitFor(helper, true))
              .thenExecuteAfter(10, () -> {
                  Recorder.mark("nbt 0");
                  Recorder.nbt(0);
                  Recorder.mark("nbt 30");
                  Recorder.nbt(30);
                  Recorder.mark("nbt 47");
                  Recorder.nbt(47);
              })
              .thenExecuteAfter(4, Recorder::finish)
              .thenSucceed()
        );
    }

    @GameTest(template = STRAIGHT_48, setupTicks = SETUP, timeoutTicks = TIMEOUT, batch = "trace_chemical_reopen")
    @TestHolder(description = "Records a whole network inside one chunk, with chemical, being saved with the chunk and loaded again.")
    public static void reopen(final DynamicTest test) {
        final ChunkData chunkData = new ChunkData(1);
        test.eventListeners().forge().addListener((final ChunkEvent.Unload event) -> chunkData.updateChunk(event, false));
        test.eventListeners().forge().addListener((final ChunkEvent.Load event) -> chunkData.updateChunk(event, true));
        listen(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  Recorder.begin("reopen", helper, CHUNKS * 16, 1);
                  int start = chunkStart(helper, 1) + 4;
                  for (int x = start; x < start + 5; x++) {
                      placeNow(helper, x == start + 2 ? advanced() : basic(), x, 0);
                  }
                  atPush(helper, 6, start, 0, HYDROGEN, 24001);
                  at(10, "nbt " + start, () -> Recorder.nbt(start));
              })
              .thenWaitUntil(() -> {
                  if (Recorder.relativeTickNow() < 14) {
                      helper.fail("Waiting for the chemical");
                  }
              })
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
              .thenExecuteAfter(12, Recorder::finish)
              .thenSucceed()
        );
    }

    /// Places a tube at the relative x, z (mark `place x z`, with the block name for the tier).
    private static void place(MekGameTestHelper helper, Block block, int x, int z) {
        helper.setBlock(x, 0, z, block.defaultBlockState());
    }

    private static void placeNow(MekGameTestHelper helper, Block block, int x, int z) {
        Recorder.mark("place " + x + " " + z + " " + tierOf(block));
        place(helper, block, x, z);
    }

    private static void tankNow(MekGameTestHelper helper, String name, int x, int y, int z, String tier, String chemical, long amount) {
        Recorder.mark("tank " + name + " " + x + " " + y + " " + z + " " + tier + " " + chemical + " " + amount);
        tank(helper, name, x, y, z, tier, chemical, amount);
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
        return block == MekanismBlocks.BASIC_PRESSURIZED_TUBE.get() ? "basic"
               : block == MekanismBlocks.ADVANCED_PRESSURIZED_TUBE.get() ? "advanced"
               : block == MekanismBlocks.ELITE_PRESSURIZED_TUBE.get() ? "elite" : "ultimate";
    }

    private static void breakAt(MekGameTestHelper helper, int x, int z) {
        helper.setBlock(x, 0, z, Blocks.AIR);
    }

    private static ChemicalResource chemicalOf(String chemical) {
        int colon = chemical.indexOf(':');
        ResourceKey<Chemical> key = ResourceKey.create(MekanismRegistries.Keys.CHEMICAL, Identifier.fromNamespaceAndPath(chemical.substring(0, colon), chemical.substring(colon + 1)));
        return ChemicalResource.of(ServerLifecycleHooks.getCurrentServer().registryAccess().getOrThrow(key));
    }

    private static BlockState tankState(String tier) {
        return switch (tier) {
            case "basic" -> MekanismBlocks.BASIC_CHEMICAL_TANK.defaultState();
            case "advanced" -> MekanismBlocks.ADVANCED_CHEMICAL_TANK.defaultState();
            case "elite" -> MekanismBlocks.ELITE_CHEMICAL_TANK.defaultState();
            case "ultimate" -> MekanismBlocks.ULTIMATE_CHEMICAL_TANK.defaultState();
            case "creative" -> MekanismBlocks.CREATIVE_CHEMICAL_TANK.defaultState();
            default -> throw new IllegalArgumentException(tier);
        };
    }

    private static ItemStack tankItem(String tier) {
        return new ItemStack(switch (tier) {
            case "basic" -> MekanismBlocks.BASIC_CHEMICAL_TANK.asItem();
            case "advanced" -> MekanismBlocks.ADVANCED_CHEMICAL_TANK.asItem();
            case "elite" -> MekanismBlocks.ELITE_CHEMICAL_TANK.asItem();
            case "ultimate" -> MekanismBlocks.ULTIMATE_CHEMICAL_TANK.asItem();
            case "creative" -> MekanismBlocks.CREATIVE_CHEMICAL_TANK.asItem();
            default -> throw new IllegalArgumentException(tier);
        });
    }

    /// A Chemical Tank of the tier at the relative x, y(0 or 1), z with the given chemical and amount (`-1` is the capacity, which is what a creative tank
    /// holds). A creative tank that holds a chemical gives an infinite supply to a tube that pulls from it. The tank is a configurable machine, so
    /// `setBlock` leaves every side `NONE`: it gets the sides of the item and then `INPUT_OUTPUT` on the six, without ejecting (decision 113).
    private static void tank(MekGameTestHelper helper, String name, int x, int y, int z, String tier, String chemical, long amount) {
        helper.setBlock(x, y, z, tankState(tier));
        TileEntityChemicalTank tile = WorldUtils.getTileEntity(TileEntityChemicalTank.class, helper.getLevel(), helper.absolutePos(new BlockPos(x, y, z)));
        int placed = capabilityMask(helper, x, y, z);
        tile.applyComponentsFromItemStack(tankItem(tier));
        int components = capabilityMask(helper, x, y, z);
        ConfigInfo info = ((ISideConfiguration) tile).getConfig().getConfig(TransmissionType.CHEMICAL);
        boolean ejecting = info.isEjecting();
        String before = describeSides(info);
        for (RelativeSide relative : RelativeSide.VALUES) {
            info.setDataType(DataType.INPUT_OUTPUT, relative);
            //What the configuration screen does after a click: the side changed, so the capability of that side is invalidated
            ((ISideConfiguration) tile).getConfig().sideChanged(TransmissionType.CHEMICAL, relative);
        }
        info.setEjecting(false);
        Recorder.event("capdiag", "\"name\":\"" + name + "\",\"placed\":" + placed + ",\"components\":" + components + ",\"io\":" + capabilityMask(helper, x, y, z)
                                 + ",\"ejecting\":" + ejecting + ",\"before\":\"" + before + "\",\"after\":\"" + describeSides(info) + "\""
                                 + ",\"ins\":" + handlerMask(helper, x, y, z, true, HYDROGEN));
        if (!chemical.equals("empty") && amount != 0) {
            ChemicalResource resource = chemicalOf(chemical);
            tile.getChemicalTank().setContents(resource, amount < 0 ? tile.getChemicalTank().capacityAsLong(resource) : amount, null);
            Recorder.event("probe", "\"name\":\"" + name + "\",\"ext\":" + handlerMask(helper, x, y, z, false, chemical));
        }
        Recorder.actor(name, new BlockPos(x, y, z));
    }

    private static String describeSides(ConfigInfo info) {
        StringBuilder text = new StringBuilder();
        for (RelativeSide relative : RelativeSide.VALUES) {
            text.append(text.isEmpty() ? "" : ",").append(info.getDataType(relative));
        }
        return text.toString();
    }

    /// The sides where a one-unit insert (or extract) of the chemical is accepted by the handler of the capability, simulated in a root transaction that is not committed.
    private static int handlerMask(MekGameTestHelper helper, int x, int y, int z, boolean insert, String chemical) {
        int mask = 0;
        for (Direction side : Direction.values()) {
            ResourceHandler<ChemicalResource> handler = helper.getLevel().getCapability(Capabilities.CHEMICAL.block(), helper.absolutePos(new BlockPos(x, y, z)), side);
            if (handler != null) {
                try (Transaction transaction = Transaction.openRoot()) {
                    int moved = insert ? handler.insert(chemicalOf(chemical), 1, transaction) : handler.extract(chemicalOf(chemical), 1, transaction);
                    if (moved > 0) {
                        mask |= 1 << side.ordinal();
                    }
                }
            }
        }
        return mask;
    }

    private static int capabilityMask(MekGameTestHelper helper, int x, int y, int z) {
        int mask = 0;
        for (Direction side : Direction.values()) {
            if (helper.getLevel().getCapability(Capabilities.CHEMICAL.block(), helper.absolutePos(new BlockPos(x, y, z)), side) != null) {
                mask |= 1 << side.ordinal();
            }
        }
        return mask;
    }

    private static void setChem(String name, String chemical, long amount) {
        IChemicalTank tank = Recorder.actorTank(name);
        if (tank == null) {
            return;
        }
        if (chemical.equals("empty") || amount == 0) {
            tank.setContents(LargeResourceStack.CHEMICAL_HELPER.empty(), null);
        } else {
            ChemicalResource resource = chemicalOf(chemical);
            tank.setContents(resource, amount < 0 ? tank.capacityAsLong(resource) : amount, null);
        }
    }

    /// What the wrench does to a side: the raw type, `onModeChange`, `refreshConnections` and `notifyTileChange`. A `PULL` side makes the tube pull from
    /// the acceptor there (`pullFromAcceptors`); `NORMAL` on a side that already is `NORMAL` just rechecks the connections.
    private static void mode(MekGameTestHelper helper, int x, int z, Direction side, ConnectionType type) {
        TileEntityPressurizedTube tile = WorldUtils.getTileEntity(TileEntityPressurizedTube.class, helper.getLevel(), helper.absolutePos(new BlockPos(x, 0, z)));
        if (tile != null) {
            PressurizedTube pipe = tile.getTransmitter();
            pipe.setConnectionTypeRaw(side, type);
            //What the recheck will see: the connections now and the ones the pipe would have, and whether it has any chemical to be the primary network
            Recorder.event("modediag", "\"x\":" + x + ",\"z\":" + z + ",\"current\":" + pipe.currentTransmitterConnections + ",\"possible\":"
                                     + pipe.getPossibleTransmitterConnections() + ",\"noBuffer\":" + pipe.noBufferOrFallback());
            pipe.onModeChange(side);
            pipe.refreshConnections();
            pipe.notifyTileChange();
        }
    }

    /// Pushes chemical into the tube at the relative x, z the way a neighbour does (`insert` of the handler the capability hands out on the first side that
    /// exposes a container, in a root transaction that is committed), and logs what it took.
    private static void push(MekGameTestHelper helper, int x, int z, String chemical, int amount) {
        BlockPos pos = helper.absolutePos(new BlockPos(x, 0, z));
        ChemicalResource resource = chemicalOf(chemical);
        int got = 0;
        String sideName = "none";
        TileEntityPressurizedTube tile = WorldUtils.getTileEntity(TileEntityPressurizedTube.class, helper.getLevel(), pos);
        for (Direction side : Direction.values()) {
            //The first side the pipe accepts on (a side with no connection hands out a handler that takes nothing)
            if (tile == null || !tile.getTransmitter().getConnectionType(side).canAccept()) {
                continue;
            }
            ResourceHandler<ChemicalResource> handler = helper.getLevel().getCapability(Capabilities.CHEMICAL.block(), pos, side);
            if (handler != null) {
                try (Transaction transaction = Transaction.openRoot()) {
                    got = handler.insert(resource, amount, transaction);
                    transaction.commit();
                }
                sideName = side.getName();
                break;
            }
        }
        Recorder.event("push", "\"x\":" + x + ",\"z\":" + z + ",\"chem\":\"" + chemical + "\",\"amount\":" + amount + ",\"got\":" + got + ",\"side\":\"" + sideName + "\"");
    }

    private static void listen(DynamicTest test) {
        test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> Recorder.pre());
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> Recorder.post());
        test.eventListeners().forge().addListener((final ChunkEvent.Load event) -> Recorder.chunk("load", event));
        test.eventListeners().forge().addListener((final ChunkEvent.Unload event) -> Recorder.chunk("unload", event));
        test.eventListeners().forge().addListener((final ChunkTicketLevelUpdatedEvent event) -> Recorder.ticket(event));
        test.eventListeners().forge().addListener((final ChemicalNetwork.ChemicalTransferEvent event) -> Recorder.transfer(event));
    }

    private static String id(ChemicalResource resource) {
        return resource.isEmpty() ? "" : resource.typeHolder().getRegisteredName();
    }

    /// The tube block entity, with a transmitter that logs `requestsUpdate` and creates the traced network.
    private static class TracedPipe extends TileEntityPressurizedTube {

        TracedPipe(Holder<Block> blockProvider, BlockPos pos, BlockState state) {
            super(blockProvider, pos, state);
            Recorder.tile(pos, System.identityHashCode(getTransmitter()));
        }

        @Override
        protected PressurizedTube createTransmitter(Holder<Block> blockProvider) {
            return new TracedTransmitter(blockProvider, this);
        }

        @Override
        public void chunkAccessibilityChange(boolean loaded) {
            Recorder.log("access", getBlockPos(), "\"loaded\":" + loaded);
            super.chunkAccessibilityChange(loaded);
        }
    }

    private static class TracedTransmitter extends PressurizedTube {

        TracedTransmitter(Holder<Block> blockProvider, TracedPipe tile) {
            super(blockProvider, tile);
        }

        @Override
        public ChemicalNetwork createNetworkByMerging(Collection<ChemicalNetwork> networks) {
            return new TracedNetwork(networks);
        }

        @Override
        public void requestsUpdate() {
            Recorder.log("update", getBlockPos(), "");
            super.requestsUpdate();
        }
    }

    private static class TracedNetwork extends ChemicalNetwork {

        private final int index;
        /// False while the constructor of [ChemicalNetwork] runs (it adopts the merged networks and fills the container before `index` is known).
        private boolean ready;

        TracedNetwork(Collection<ChemicalNetwork> from) {
            super(from);
            index = Recorder.nextNetworkIndex();
            ready = true;
            StringBuilder merged = new StringBuilder();
            for (ChemicalNetwork network : from) {
                merged.append(merged.isEmpty() ? "" : ",").append(network instanceof TracedNetwork traced ? traced.index : -1);
            }
            Recorder.event("created", "\"net\":" + index + ",\"hash\":" + getUUID().hashCode() + ",\"from\":[" + merged + "]");
        }

        /// The index of this network, which the constructor of [ChemicalNetwork] cannot read yet: it is the next one to be handed out.
        private int idx() {
            return ready ? index : Recorder.peekNetworkIndex();
        }

        @Override
        public List<PressurizedTube> adoptTransmittersAndAcceptorsFrom(ChemicalNetwork net) {
            if (ready) {
                Recorder.event("adopt", "\"net\":" + index + ",\"from\":" + (net instanceof TracedNetwork traced ? traced.index : -1));
            }
            return super.adoptTransmittersAndAcceptorsFrom(net);
        }

        @Override
        protected void disperse(PressurizedTube triggerTransmitter, ChemicalResource resource, long amount) {
            if (ready) {
                Recorder.disperse(triggerTransmitter.getBlockPos(), "\"net\":" + index + ",\"chem\":\"" + id(resource) + "\",\"amount\":" + amount);
            }
            super.disperse(triggerTransmitter, resource, amount);
        }

        @Override
        public void onUpdate() {
            Recorder.event("net_update", "\"net\":" + index);
            super.onUpdate();
        }

        @Override
        public void onContentsChanged() {
            if (container != null) {
                Recorder.event("contents", "\"net\":" + idx() + ",\"chem\":\"" + id(container.resource()) + "\",\"amount\":" + container.amountAsLong());
            }
            super.onContentsChanged();
        }

        @Override
        public void commit() {
            Recorder.event("commit", "\"net\":" + index);
            super.commit();
        }

        @Override
        public void invalidate(@Nullable PressurizedTube triggerTransmitter) {
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

        /// Puts [TracedPipe] in place of the factory of the four tube block entity types, as a no-op while nothing is recording.
        static synchronized void installTracedPipe() {
            if (installed) {
                return;
            }
            try {
                Field factory = BlockEntityType.class.getDeclaredField("factory");
                factory.setAccessible(true);
                install(factory, MekanismTileEntityTypes.BASIC_PRESSURIZED_TUBE.get(), MekanismBlocks.BASIC_PRESSURIZED_TUBE);
                install(factory, MekanismTileEntityTypes.ADVANCED_PRESSURIZED_TUBE.get(), MekanismBlocks.ADVANCED_PRESSURIZED_TUBE);
                install(factory, MekanismTileEntityTypes.ELITE_PRESSURIZED_TUBE.get(), MekanismBlocks.ELITE_PRESSURIZED_TUBE);
                install(factory, MekanismTileEntityTypes.ULTIMATE_PRESSURIZED_TUBE.get(), MekanismBlocks.ULTIMATE_PRESSURIZED_TUBE);
                installed = true;
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private static void install(Field factory, BlockEntityType<TileEntityPressurizedTube> type, Holder<Block> provider) throws IllegalAccessException {
            BlockEntityType.BlockEntitySupplier<TileEntityPressurizedTube> traced = (pos, state) -> new TracedPipe(provider, pos, state);
            factory.set(type, traced);
        }

        private final java.util.Map<String, BlockPos> actors = new java.util.LinkedHashMap<>();

        static void actor(String name, BlockPos relative) {
            Recorder recorder = current;
            if (recorder != null) {
                recorder.actors.put(name, relative);
            }
        }

        /// The chemical tank of an actor: the tank of a Chemical Tank.
        @Nullable
        static IChemicalTank actorTank(String name) {
            Recorder recorder = current;
            BlockPos relative = recorder == null ? null : recorder.actors.get(name);
            return relative == null ? null : recorder.tankAt(relative);
        }

        @Nullable
        private IChemicalTank tankAt(BlockPos relative) {
            BlockPos pos = origin.offset(relative.getX(), relative.getY(), relative.getZ());
            if (level.getBlockEntity(pos) instanceof TileEntityChemicalTank tank) {
                return tank.getChemicalTank();
            }
            return null;
        }

        static long relativeTickNow() {
            Recorder recorder = current;
            return recorder == null ? 0 : recorder.relativeTick();
        }

        static void transfer(ChemicalNetwork.ChemicalTransferEvent event) {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished && event.network instanceof TracedNetwork traced) {
                recorder.add("transfer", "\"net\":" + traced.index + ",\"scale\":" + event.network.currentScale + ",\"chem\":\"" + id(event.transferType) + "\"");
            }
        }

        /// What `PressurizedTube.write` saves (it validates the save shares first, once per game tick).
        static void nbt(int x) {
            Recorder recorder = current;
            if (recorder == null || recorder.finished) {
                return;
            }
            TileEntityPressurizedTube tile = recorder.pipe(x);
            if (tile == null) {
                recorder.add("nbt", "\"x\":" + x + ",\"chem\":\"\",\"amount\":-1");
                return;
            }
            TagValueOutput out = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, recorder.level.registryAccess());
            tile.getTransmitter().write(out);
            CompoundTag tag = out.buildResult();
            ValueInput input = TagValueInput.create(ProblemReporter.DISCARDING, recorder.level.registryAccess(), tag);
            LargeResourceStack<ChemicalResource> stored = LargeResourceStack.CHEMICAL_HELPER.readOrEmpty(input, SerializationConstants.STORED);
            recorder.add("nbt", "\"x\":" + x + ",\"chem\":\"" + id(stored.resource()) + "\",\"amount\":" + stored.amount());
        }

        @SuppressWarnings("unchecked")
        private static LargeResourceStack<ChemicalResource> saveShare(PressurizedTube pipe) {
            try {
                Field journal = BufferedResourceTransmitter.class.getDeclaredField("saveShareJournal");
                journal.setAccessible(true);
                Object value = journal.get(pipe);
                Field share = value.getClass().getDeclaredField("saveShare");
                share.setAccessible(true);
                return (LargeResourceStack<ChemicalResource>) share.get(value);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        /// The chemical of every tube, network and actor after `onTick`.
        private void recordChemical(Map<Integer, TracedNetwork> nets) {
            StringBuilder pipes = new StringBuilder();
            for (int x = 0; x < width * depth; x++) {
                TileEntityPressurizedTube tile = pipe(x);
                if (tile == null) {
                    continue;
                }
                PressurizedTube pipe = tile.getTransmitter();
                LargeResourceStack<ChemicalResource> buffer = pipe.getShare();
                LargeResourceStack<ChemicalResource> share = saveShare(pipe);
                pipes.append(pipes.isEmpty() ? "" : ",").append("{\"x\":").append(x)
                      .append(",\"buf\":").append(buffer.amount())
                      .append(",\"chem\":\"").append(id(buffer.resource())).append('"')
                      .append(",\"share\":").append(share.amount())
                      .append(",\"shareChem\":\"").append(id(share.resource())).append('"')
                      .append(",\"cap\":").append(pipe.getCapacity()).append('}');
            }
            StringBuilder networks = new StringBuilder();
            for (Map.Entry<Integer, TracedNetwork> entry : nets.entrySet()) {
                TracedNetwork network = entry.getValue();
                networks.append(networks.isEmpty() ? "" : ",").append("{\"net\":").append(entry.getKey())
                      .append(",\"chem\":\"").append(id(network.getContainer().resource())).append('"')
                      .append(",\"amount\":").append(network.getContainer().amountAsLong())
                      .append(",\"capacity\":").append(network.getCapacity())
                      .append(",\"scale\":").append(network.currentScale)
                      .append(",\"scaleBits\":").append(Float.floatToRawIntBits(network.currentScale))
                      .append(",\"prev\":").append(network.getPrevTransferAmount())
                      .append(",\"last\":\"").append(id(network.getLastType())).append("\"}");
            }
            StringBuilder actorList = new StringBuilder();
            for (Map.Entry<String, BlockPos> entry : actors.entrySet()) {
                IChemicalTank tank = tankAt(entry.getValue());
                actorList.append(actorList.isEmpty() ? "" : ",").append("{\"name\":\"").append(entry.getKey()).append("\",\"chem\":\"")
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

        /// What happened before `begin`, for the structure being built: tiles and networks, with the absolute position of the tubes.
        private record Before(long tick, String kind, @Nullable BlockPos pos, String fields) {
        }

        private static final List<Before> BEFORE = new ArrayList<>();

        static synchronized void before(String kind, @Nullable BlockPos pos, String fields) {
            BEFORE.add(new Before(ServerLifecycleHooks.getCurrentServer().getTickCount(), kind, pos, fields));
            if (BEFORE.size() > 20000) {
                BEFORE.subList(0, 5000).clear();
            }
        }

        /// A line (`depth` 1) or a plane of tubes; the tube at `x`, `z` has the index `z * width + x`.
        static void begin(String name, MekGameTestHelper helper, int width, int depth) {
            Recorder recorder = new Recorder(name, helper, width, depth);
            StringBuilder tiles = new StringBuilder();
            for (int x = 0; x < width * depth; x++) {
                TileEntityPressurizedTube tile = recorder.pipe(x);
                if (tile != null) {
                    tiles.append(tiles.isEmpty() ? "" : ",").append("{\"x\":").append(x).append(",\"hash\":")
                          .append(System.identityHashCode(tile.getTransmitter())).append('}');
                }
            }
            //How many block entities each chunk of the line already holds: the order in which a chunk unloads them is the order of its hash table, which
            //depends on how many it has held (leftovers of other tests in the same chunk change it), so the trace says what the chunk had
            StringBuilder chunkEntities = new StringBuilder();
            for (int cx = 0; cx <= CHUNKS; cx++) {
                LevelChunk chunk = recorder.level.getChunkSource().getChunkNow(recorder.originChunk.x() + cx, recorder.originChunk.z());
                chunkEntities.append(cx == 0 ? "" : ",").append(chunk == null ? -1 : chunk.getBlockEntities().size());
            }
            recorder.add("setup", "\"x\":" + recorder.origin.getX() + ",\"y\":" + recorder.origin.getY() + ",\"z\":" + recorder.origin.getZ()
                                  + ",\"width\":" + width + ",\"depth\":" + depth + ",\"xInChunk\":" + (recorder.origin.getX() & 15) + ",\"tiles\":[" + tiles + "]"
                                  + ",\"chunkEntities\":[" + chunkEntities + "]");
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
            } else if (!kind.equals("net_update") && !kind.equals("contents") && !kind.equals("adopt") && !kind.equals("capdiag") && !kind.equals("probe") && !kind.equals("push")) {
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

        /// A `disperse` of a tube of this structure; nothing before `begin`, because it is not part of how the structure was built.
        static void disperse(BlockPos pos, String extra) {
            Recorder recorder = current;
            if (recorder != null && !recorder.finished) {
                int x = recorder.index(pos);
                if (x >= 0) {
                    recorder.add("disperse", "\"x\":" + x + "," + extra);
                }
            }
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
        private TileEntityPressurizedTube pipe(int x) {
            return WorldUtils.getTileEntity(TileEntityPressurizedTube.class, level, origin.offset(x % width, 0, x / width), false);
        }

        /// The index of the tube at `pos` (`z * width + x`), or -1 when the position is not in the structure.
        private int index(BlockPos pos) {
            int x = pos.getX() - origin.getX();
            int z = pos.getZ() - origin.getZ();
            return x >= 0 && x < width && z >= 0 && z < depth && pos.getY() == origin.getY() ? z * width + x : -1;
        }

        private long relativeTick() {
            return ServerLifecycleHooks.getCurrentServer().getTickCount() - startTick;
        }

        /// The registry after `onTick`: every tube, every network that has a tube of the line, and the order of the registry set.
        private void recordState() {
            Map<Integer, TracedNetwork> nets = new TreeMap<>();
            StringBuilder cables = new StringBuilder();
            for (int x = 0; x < width * depth; x++) {
                cables.append(x == 0 ? "" : ",");
                TileEntityPressurizedTube tile = pipe(x);
                if (tile == null) {
                    cables.append("null");
                    continue;
                }
                PressurizedTube pipe = tile.getTransmitter();
                ChemicalNetwork network = pipe.getTransmitterNetwork();
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
                for (PressurizedTube member : network.getTransmitters()) {
                    members.append(members.isEmpty() ? "" : ",").append(index(member.getBlockPos()));
                }
                networkList.append(networkList.isEmpty() ? "" : ",").append("{\"net\":").append(entry.getKey())
                      .append(",\"hash\":").append(network.getUUID().hashCode())
                      .append(",\"size\":").append(network.transmittersSize())
                      .append(",\"acceptors\":").append(network.getAcceptorCount())
                      .append(",\"transmitters\":[").append(members).append("]}");
            }
            add("state", "\"cables\":[" + cables + "],\"nets\":[" + networkList + "],\"registry\":[" + registry + "]");
            recordChemical(nets);
            flush();
        }

        private boolean inLine(TracedNetwork network) {
            for (PressurizedTube member : network.getTransmitters()) {
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
                Path file = Path.of("trace", "chemical_" + name + ".jsonl");
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
