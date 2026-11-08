package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.function.BooleanSupplier;
import java.util.function.Supplier;
import mekanism.api.MekanismRegistries;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.chemical.IChemicalTank;
import mekanism.api.upgrade.IUpgradeHelper;
import mekanism.api.upgrade.Upgrade;
import mekanism.api.upgrade.UpgradeIds;
import mekanism.api.resource.LargeResourceStack;
import mekanism.common.inventory.container.MekanismContainer;
import mekanism.common.inventory.container.SelectedWindowData;
import mekanism.common.inventory.container.SelectedWindowData.WindowType;
import mekanism.common.inventory.container.sync.ISyncableData;
import mekanism.common.inventory.container.tile.MekanismTileContainer;
import mekanism.common.network.to_client.container.property.PropertyData;
import mekanism.common.network.to_server.PacketGuiInteract;
import mekanism.common.network.to_server.PacketGuiInteract.GuiInteraction;
import mekanism.common.network.to_server.PacketWindowSelect;
import mekanism.common.registration.impl.ContainerTypeRegistryObject;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismContainerTypes;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.interfaces.IRedstoneControl.RedstoneControl;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import mekanism.common.tile.prefab.TileEntityRecipeMachine;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.registries.GeneratorsContainerTypes;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.RegistryAccess;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.network.handling.IPayloadContext;
import net.neoforged.neoforge.transfer.energy.EnergyHandler;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.resource.RegisteredResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava, nos cinco contêineres das máquinas portadas (Enrichment Chamber, Crusher, Energized Smelter, Metallurgic Infuser e Heat
/// Generator), o que o `MekanismContainer.broadcastChanges` mandaria a cada tick a um jogador com a tela aberta, e o que o
/// `sendAllDataToRemote` manda ao abrir. O jogador é um `ServerPlayer` falso (`makeMockPlayerLookingAt`), e o menu NÃO é o
/// `containerMenu` dele fora dos pacotes: ninguém o transmite sozinho, então o teste chama o `broadcastChanges` e o
/// `sendAllDataToRemote` reais do `MekanismContainer` uma vez por tick, em `ServerTickEvent.Post`, como o `ServerPlayer.tick` do tick
/// seguinte os chamaria (o estado entre os dois é o mesmo). Cada `ISyncableData` é trocado por um espião só durante a chamada, para o
/// teste ver o que o laço do original pede (`getPropertyData` só roda nos sujos) sem mudar o resultado; o `PacketUpdateContainer` sai
/// para a conexão falsa. A lista de dados de `startTrackingServer` (a janela de upgrades) não passa pelo laço do menu: o teste lê o
/// que o `sendInitialDataToRemote` leu, logo depois da chamada.
///
/// Uma linha por tick: `sent` (o que o `sendAllDataToRemote` e o `startTrackingServer` mandaram nesse tick) e `dirty` (o que o
/// `broadcastChanges` mandou), cada entrada `[índice, classe, "DIRTY"|"SIZE", valor]`; o valor de um `ByteArray` é hexadecimal e o de
/// uma pilha, `{"resource", "amount"}`.
@ForEachTest(groups = "trace")
public class TrackerTraceTest {

    private static final BlockPos POS = new BlockPos(1, 0, 0);
    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    /// Uma operação de receita leva 200 ticks nas quatro máquinas, mais a folga para o fim dela e para o resultado sair.
    private static final int RUN = 215;

    private static void template(DynamicTest test, Supplier<BlockState> state) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(2, 1, 1).fill(1, 0, 0, 1, 0, 0, state.get()));
    }

    // -------------------------------------------------------------------------------------------------------- máquinas elétricas

    private static void electric(DynamicTest test, String name, Supplier<BlockState> block, ContainerTypeRegistryObject<?> type, Item item, int count) {
        template(test, block);
        Track track = new Track(name, type);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> track.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> track.start(helper))
              .thenExecuteAfter(3, () -> {
                  track.mark("open");
                  track.open();
              })
              .thenExecuteAfter(3, () -> {
                  track.mark("insert 15");
                  track.insert(15);
              })
              .thenExecuteAfter(1, () -> {
                  track.mark("insert 10");
                  track.insert(10);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("energy 20000");
                  track.setEnergy(20_000);
              })
              .thenExecuteAfter(3, () -> {
                  track.mark("item " + BuiltInId.of(item) + " " + count);
                  track.setSlot(0, item, count);
              })
              .thenExecuteAfter(30, () -> {
                  track.mark("insert 1000");
                  track.insert(1_000);
              })
              .thenExecuteAfter(RUN, () -> {
                  track.mark("mode high");
                  track.setMode(RedstoneControl.HIGH);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal off");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("mode low");
                  track.setMode(RedstoneControl.LOW);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal off, mode pulse");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
                  track.setMode(RedstoneControl.PULSE);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal off, mode disabled");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
                  track.setMode(RedstoneControl.DISABLED);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("item " + BuiltInId.of(item) + " " + count + ", energy 20000");
                  track.setSlot(0, item, count);
                  track.setEnergy(20_000);
              })
              .thenExecuteAfter(10, () -> {
                  track.mark("slot speed 2");
                  track.clickUpgradeSlot(UpgradeIds.SPEED, 2);
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("window upgrade");
                  track.openUpgradeWindow();
              })
              .thenExecuteAfter(5, () -> {
                  track.mark("slot energy 2");
                  track.clickUpgradeSlot(UpgradeIds.ENERGY, 2);
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("slot speed 1");
                  track.clickUpgradeSlot(UpgradeIds.SPEED, 1);
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("window upgrade again");
                  track.openUpgradeWindow();
              })
              .thenExecuteAfter(10, () -> {
                  track.mark("stop tracking");
                  track.closeUpgradeWindow();
              })
              .thenExecuteAfter(5, () -> {
                  track.mark("slot energy 1");
                  track.clickUpgradeSlot(UpgradeIds.ENERGY, 1);
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("close");
                  track.close();
              })
              .thenExecuteAfter(5, track::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tracker")
    @TestHolder(description = "Records what the Enrichment Chamber container sends a player every tick: opening, a recipe to the end, energy arriving, redstone modes, upgrades and closing.")
    public static void chamberTrackers(final DynamicTest test) {
        electric(test, "chamber_trackers", MekanismBlocks.ENRICHMENT_CHAMBER::defaultState, MekanismContainerTypes.ENRICHMENT_CHAMBER, Items.CLAY, 1);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tracker")
    @TestHolder(description = "Records the same for the Crusher.")
    public static void crusherTrackers(final DynamicTest test) {
        electric(test, "crusher_trackers", MekanismBlocks.CRUSHER::defaultState, MekanismContainerTypes.CRUSHER, Items.COBBLESTONE, 1);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tracker")
    @TestHolder(description = "Records the same for the Energized Smelter.")
    public static void smelterTrackers(final DynamicTest test) {
        electric(test, "smelter_trackers", MekanismBlocks.ENERGIZED_SMELTER::defaultState, MekanismContainerTypes.ENERGIZED_SMELTER, Items.RAW_IRON, 1);
    }

    // ------------------------------------------------------------------------------------------------------- Metallurgic Infuser

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tracker")
    @TestHolder(description = "Records the same for the Metallurgic Infuser, with the chemical tank changing too.")
    public static void infuserTrackers(final DynamicTest test) {
        template(test, MekanismBlocks.METALLURGIC_INFUSER::defaultState);
        Track track = new Track("infuser_trackers", MekanismContainerTypes.METALLURGIC_INFUSER);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> track.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> track.start(helper))
              .thenExecuteAfter(3, () -> {
                  track.mark("open");
                  track.open();
              })
              .thenExecuteAfter(3, () -> {
                  track.mark("insert 15");
                  track.insert(15);
              })
              .thenExecuteAfter(1, () -> {
                  track.mark("insert 10");
                  track.insert(10);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("energy 20000");
                  track.setEnergy(20_000);
              })
              .thenExecuteAfter(3, () -> {
                  track.mark("item copper_ingot 1, tank redstone 30");
                  track.setSlot(1, Items.COPPER_INGOT, 1);
                  track.setTank(30);
              })
              .thenExecuteAfter(30, () -> {
                  track.mark("insert 1000");
                  track.insert(1_000);
              })
              .thenExecuteAfter(RUN, () -> {
                  track.mark("tank redstone 0");
                  track.setTank(0);
              })
              .thenExecuteAfter(10, () -> {
                  track.mark("infusion redstone 3");
                  track.setSlot(0, Items.REDSTONE, 3);
              })
              .thenExecuteAfter(30, () -> {
                  track.mark("mode high");
                  track.setMode(RedstoneControl.HIGH);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal off");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("mode low");
                  track.setMode(RedstoneControl.LOW);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal off, mode pulse");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
                  track.setMode(RedstoneControl.PULSE);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal off, mode disabled");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
                  track.setMode(RedstoneControl.DISABLED);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("item copper_ingot 1, tank redstone 30, energy 20000");
                  track.setSlot(1, Items.COPPER_INGOT, 1);
                  track.setTank(30);
                  track.setEnergy(20_000);
              })
              .thenExecuteAfter(10, () -> {
                  track.mark("slot speed 2");
                  track.clickUpgradeSlot(UpgradeIds.SPEED, 2);
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("window upgrade");
                  track.openUpgradeWindow();
              })
              .thenExecuteAfter(5, () -> {
                  track.mark("slot energy 2");
                  track.clickUpgradeSlot(UpgradeIds.ENERGY, 2);
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("slot speed 1");
                  track.clickUpgradeSlot(UpgradeIds.SPEED, 1);
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("window upgrade again");
                  track.openUpgradeWindow();
              })
              .thenExecuteAfter(10, () -> {
                  track.mark("stop tracking");
                  track.closeUpgradeWindow();
              })
              .thenExecuteAfter(5, () -> {
                  track.mark("slot energy 1");
                  track.clickUpgradeSlot(UpgradeIds.ENERGY, 1);
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("close");
                  track.close();
              })
              .thenExecuteAfter(5, track::write)
              .thenSucceed()
        );
    }

    // ----------------------------------------------------------------------------------------------------------- Heat Generator

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tracker")
    @TestHolder(description = "Records the same for the Heat Generator: lava burning, the heat and the losses every tick, fuel in the slot, redstone modes, a muffling upgrade and closing.")
    public static void heatGeneratorTrackers(final DynamicTest test) {
        template(test, GeneratorsBlocks.HEAT_GENERATOR::defaultState);
        Track track = new Track("heat_generator_trackers", GeneratorsContainerTypes.HEAT_GENERATOR);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> track.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> track.start(helper))
              .thenExecuteAfter(3, () -> {
                  track.mark("open");
                  track.open();
              })
              .thenExecuteAfter(3, () -> {
                  track.mark("lava 500");
                  track.setLava(500);
              })
              .thenExecuteAfter(60, () -> {
                  track.mark("energy 235");
                  track.setEnergy(235);
              })
              .thenExecuteAfter(20, () -> {
                  track.mark("energy 0, fuel coal 2");
                  track.setEnergy(0);
                  track.setSlot(0, Items.COAL, 2);
              })
              .thenExecuteAfter(80, () -> {
                  track.mark("lava 0");
                  track.setLava(0);
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("lava 500");
                  track.setLava(500);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("mode high");
                  track.setMode(RedstoneControl.HIGH);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal off");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("mode low");
                  track.setMode(RedstoneControl.LOW);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal off, mode pulse");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
                  track.setMode(RedstoneControl.PULSE);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal on");
                  helper.setBlock(0, 0, 0, Blocks.REDSTONE_BLOCK);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("signal off, mode disabled");
                  helper.setBlock(0, 0, 0, Blocks.AIR);
                  track.setMode(RedstoneControl.DISABLED);
              })
              .thenExecuteAfter(10, () -> {
                  track.mark("slot muffling 1");
                  track.clickUpgradeSlot(UpgradeIds.MUFFLING, 1);
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("window upgrade");
                  track.openUpgradeWindow();
              })
              .thenExecuteAfter(5, () -> {
                  track.mark("slot muffling 1");
                  track.clickUpgradeSlot(UpgradeIds.MUFFLING, 1);
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("stop tracking");
                  track.closeUpgradeWindow();
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("close");
                  track.close();
              })
              .thenExecuteAfter(5, track::write)
              .thenSucceed()
        );
    }


    // ------------------------------------------------------------------------------------------------- tile reaberto antes do primeiro tick

    /// O mundo salvo e reaberto no meio de uma operação, com a tela aberta antes do primeiro tick do tile novo: o `sendAllDataToRemote` lê o tile que o
    /// `loadAdditional` carregou do NBT, e o `broadcastChanges` dos ticks seguintes mostra o que o primeiro tick muda.
    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tracker")
    @TestHolder(description = "Records what the Enrichment Chamber container sends when the screen opens on a tile just loaded from the saved NBT, before its first tick.")
    public static void chamberReloaded(final DynamicTest test) {
        template(test, MekanismBlocks.ENRICHMENT_CHAMBER::defaultState);
        Track track = new Track("chamber_trackers_reloaded", MekanismContainerTypes.ENRICHMENT_CHAMBER);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> track.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> track.start(helper))
              .thenExecuteAfter(3, () -> {
                  track.mark("open");
                  track.open();
              })
              .thenExecuteAfter(3, () -> {
                  track.mark("energy 20000");
                  track.setEnergy(20_000);
              })
              .thenExecuteAfter(1, () -> {
                  track.mark("item clay 1");
                  track.setSlot(0, Items.CLAY, 1);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("slot speed 2");
                  track.clickUpgradeSlot(UpgradeIds.SPEED, 2);
              })
              .thenExecuteAfter(45, () -> {
                  track.mark("mode low");
                  track.setMode(RedstoneControl.LOW);
              })
              .thenExecuteAfter(2, () -> {
                  track.mark("close");
                  track.close();
              })
              .thenExecuteAfter(2, () -> {
                  track.mark("reload");
                  track.reload();
                  track.mark("open");
                  track.open();
              })
              .thenExecuteAfter(5, () -> {
                  track.mark("window upgrade");
                  track.openUpgradeWindow();
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("energy 0");
                  track.setEnergy(0);
              })
              .thenExecuteAfter(20, () -> {
                  track.mark("close");
                  track.close();
              })
              .thenExecuteAfter(5, track::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tracker")
    @TestHolder(description = "Records the same for the Metallurgic Infuser, with the chemical tank and the upgrades of the loaded tile.")
    public static void infuserReloaded(final DynamicTest test) {
        template(test, MekanismBlocks.METALLURGIC_INFUSER::defaultState);
        Track track = new Track("infuser_trackers_reloaded", MekanismContainerTypes.METALLURGIC_INFUSER);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> track.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> track.start(helper))
              .thenExecuteAfter(3, () -> {
                  track.mark("open");
                  track.open();
              })
              .thenExecuteAfter(3, () -> {
                  track.mark("energy 20000");
                  track.setEnergy(20_000);
              })
              .thenExecuteAfter(1, () -> {
                  track.mark("item copper_ingot 1, tank redstone 30");
                  track.setSlot(1, Items.COPPER_INGOT, 1);
                  track.setTank(30);
              })
              .thenExecuteAfter(4, () -> {
                  track.mark("slot speed 2");
                  track.clickUpgradeSlot(UpgradeIds.SPEED, 2);
              })
              .thenExecuteAfter(45, () -> {
                  track.mark("mode low");
                  track.setMode(RedstoneControl.LOW);
              })
              .thenExecuteAfter(2, () -> {
                  track.mark("close");
                  track.close();
              })
              .thenExecuteAfter(2, () -> {
                  track.mark("reload");
                  track.reload();
                  track.mark("open");
                  track.open();
              })
              .thenExecuteAfter(5, () -> {
                  track.mark("window upgrade");
                  track.openUpgradeWindow();
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("tank redstone 0");
                  track.setTank(0);
              })
              .thenExecuteAfter(20, () -> {
                  track.mark("close");
                  track.close();
              })
              .thenExecuteAfter(5, track::write)
              .thenSucceed()
        );
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tracker")
    @TestHolder(description = "Records the same for the Heat Generator: the heat, the lava and the energy of the loaded tile, with production and losses at zero.")
    public static void heatGeneratorReloaded(final DynamicTest test) {
        template(test, GeneratorsBlocks.HEAT_GENERATOR::defaultState);
        Track track = new Track("heat_generator_trackers_reloaded", GeneratorsContainerTypes.HEAT_GENERATOR);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> track.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> track.start(helper))
              .thenExecuteAfter(3, () -> {
                  track.mark("open");
                  track.open();
              })
              .thenExecuteAfter(3, () -> {
                  track.mark("lava 500");
                  track.setLava(500);
              })
              .thenExecuteAfter(30, () -> {
                  track.mark("slot muffling 1");
                  track.clickUpgradeSlot(UpgradeIds.MUFFLING, 1);
              })
              .thenExecuteAfter(45, () -> {
                  track.mark("mode low");
                  track.setMode(RedstoneControl.LOW);
              })
              .thenExecuteAfter(2, () -> {
                  track.mark("close");
                  track.close();
              })
              .thenExecuteAfter(2, () -> {
                  track.mark("reload");
                  track.reload();
                  track.mark("open");
                  track.open();
              })
              .thenExecuteAfter(5, () -> {
                  track.mark("window upgrade");
                  track.openUpgradeWindow();
              })
              .thenExecuteAfter(40, () -> {
                  track.mark("lava 0");
                  track.setLava(0);
              })
              .thenExecuteAfter(20, () -> {
                  track.mark("close");
                  track.close();
              })
              .thenExecuteAfter(5, track::write)
              .thenSucceed()
        );
    }

    // ------------------------------------------------------------------------------------------------------------------ gravação

    private static final class BuiltInId {

        static String of(Item item) {
            return net.minecraft.core.registries.BuiltInRegistries.ITEM.getKey(item).getPath();
        }
    }

    /// O espião de um `ISyncableData`: devolve o que o dado devolve e anota cada `getPropertyData`, que é o que o laço do
    /// `broadcastChanges` pede dos sujos.
    private static final class Spy implements ISyncableData {

        private final ISyncableData delegate;
        private final Track track;

        Spy(ISyncableData delegate, Track track) {
            this.delegate = delegate;
            this.track = track;
        }

        @Override
        public DirtyType isDirty() {
            return delegate.isDirty();
        }

        @Override
        public PropertyData getPropertyData(RegistryAccess registryAccess, short property, DirtyType dirtyType) {
            PropertyData data = delegate.getPropertyData(registryAccess, property, dirtyType);
            track.log(property, delegate, dirtyType, data);
            return data;
        }
    }

    private static final class Track {

        private final String name;
        private final ContainerTypeRegistryObject<?> type;
        private final List<String> lines = new ArrayList<>();
        private final List<String> marks = new ArrayList<>();
        private final List<String> sent = new ArrayList<>();
        private final List<String> dirty = new ArrayList<>();
        private List<String> sink = dirty;
        private MekGameTestHelper helper;
        private Player player;
        private MekanismTileContainer<TileEntityMekanism> menu;
        private int tick;
        private String error;
        private BooleanSupplier recheck;
        private int recheckOffset = -1;
        private boolean reloaded;

        Track(String name, ContainerTypeRegistryObject<?> type) {
            this.name = name;
            this.type = type;
        }

        void start(MekGameTestHelper helper) {
            this.helper = helper;
            // O bloco do molde não tem lado nenhum aceitando energia (a configuração de lados começa em NONE); um jogador a muda pelo
            // botão da configuração. Aqui todos os lados passam a ser de entrada, para a energia entrar pela capability, como entra de um cabo.
            if (tile() instanceof mekanism.common.tile.interfaces.ISideConfiguration sided) {
                mekanism.common.tile.component.config.ConfigInfo energy = sided.getConfig().getConfig(mekanism.common.lib.transmitter.TransmissionType.ENERGY);
                for (mekanism.api.RelativeSide side : mekanism.api.RelativeSide.values()) {
                    energy.setDataType(mekanism.common.tile.component.config.DataType.INPUT, side);
                }
            }
        }

        void mark(String mark) {
            marks.add(mark);
        }

        TileEntityMekanism tile() {
            return (TileEntityMekanism) helper.getLevel().getBlockEntity(helper.absolutePos(POS));
        }

        Player player() {
            if (player == null) {
                player = helper.makeMockPlayerLookingAt(POS, Direction.NORTH);
            }
            return player;
        }

        private RegistryAccess access() {
            return helper.getLevel().registryAccess();
        }

        private Holder<Upgrade> upgrade(ResourceKey<Upgrade> key) {
            return access().lookupOrThrow(MekanismRegistries.Keys.UPGRADES).getOrThrow(key);
        }

        // ---- o que o cenário faz

        void setEnergy(long energy) {
            tile().getEnergyContainer().setEnergy(energy, null);
        }

        /// Energia pela capability de energia do bloco, como um cabo a entrega: passa pelo `TrackingEnergyHandler` e atualiza o
        /// `LastEnergyTracker`.
        void insert(int amount) {
            // O primeiro lado cuja capability deixa entrar algo: a configuração de lados decide, e nenhum é o padrão do teste.
            for (Direction candidate : new Direction[]{Direction.UP, Direction.DOWN, Direction.NORTH, Direction.SOUTH, Direction.WEST, Direction.EAST, null}) {
                EnergyHandler handler = mekanism.common.tile.base.CapabilityTileEntity.ENERGY_HANDLER_PROVIDER.getCapability(tile(), candidate);
                if (handler == null) {
                    continue;
                }
                try (Transaction transaction = Transaction.openRoot()) {
                    int inserted = handler.insert(amount, transaction);
                    if (inserted > 0) {
                        transaction.commit();
                        mark("inserted " + inserted + " from " + candidate);
                        return;
                    }
                }
            }
            mark("inserted 0");
        }

        void setSlot(int slot, Item item, int count) {
            tile().getInventorySlots().get(slot).setContents(ItemResource.of(item), count, null);
        }

        void setMode(RedstoneControl mode) {
            tile().setControlType(mode);
        }

        void setTank(long amount) {
            IChemicalTank tank = ((TileEntityMetallurgicInfuser) tile()).infusionTank;
            Holder<Chemical> redstone = access().getOrThrow(ChemicalIds.REDSTONE);
            tank.setContents(amount == 0 ? ChemicalResource.EMPTY : ChemicalResource.of(redstone), amount, null);
        }

        void setLava(long amount) {
            ((TileEntityHeatGenerator) tile()).lavaTank.setContents(amount == 0 ? FluidResource.EMPTY : FluidResource.of(Fluids.LAVA), amount, null);
        }

        /// O jogador com o upgrade na mão clica no slot virtual: o caminho do `AbstractContainerMenu.clicked`.
        void clickUpgradeSlot(ResourceKey<Upgrade> key, int count) {
            menu.setCarried(IUpgradeHelper.INSTANCE.asResource(upgrade(key)).toStack(count));
            menu.clicked(menu.getUpgradeSlot().index, 0, ContainerInput.PICKUP, player);
            menu.setCarried(ItemStack.EMPTY);
        }

        /// Cria o menu como o servidor faz ao abrir (o construtor põe o jogador em `playersUsing`) e manda tudo, como o
        /// `AbstractContainerMenu.initMenu` faz com o `sendAllDataToRemote`.
        void open() {
            Player who = player();
            menu = new MekanismTileContainer<>(type, 100, who.getInventory(), tile());
            sink = sent;
            withSpies(menu::sendAllDataToRemote);
            sink = dirty;
        }

        void close() {
            menu.removed(player);
            menu = null;
        }

        /// O que o jogo faz ao descarregar e carregar o chunk: grava o tile e cria outro do NBT (`BlockEntity.loadStatic`, que chama o `loadAdditional`), como o
        /// `ConstantUsageTraceTest.reload`. O tile novo sorteia outro `recheckOffset`, que a próxima linha grava. Sem menu aberto.
        void reload() {
            TileEntityMekanism old = tile();
            BlockPos pos = helper.absolutePos(POS);
            net.minecraft.server.level.ServerLevel level = helper.getLevel();
            CompoundTag tag = old.saveWithFullMetadata(access());
            BlockEntity loaded = BlockEntity.loadStatic(pos, level.getBlockState(pos), tag, access());
            if (loaded == null || loaded == old) {
                throw new IllegalStateException("loadStatic gave " + loaded);
            }
            LevelChunk chunk = level.getChunkAt(pos);
            old.onChunkUnloaded();
            chunk.removeBlockEntity(pos);
            chunk.addAndRegisterBlockEntity(loaded);
            recheck = null;
            recheckOffset = -1;
            reloaded = true;
        }

        /// `PacketWindowSelect` e `PacketGuiInteract.CONTAINER_TRACK_UPGRADES`, os reais, com o menu como o `containerMenu` do jogador
        /// só durante o pacote. O `startTrackingServer` manda os dois dados novos; aqui eles são lidos logo depois, com o índice que
        /// o `sendInitialDataToRemote` usou.
        void openUpgradeWindow() {
            int before = trackedData().size();
            packet(() -> {
                new PacketWindowSelect(new SelectedWindowData(WindowType.UPGRADE)).handle(context());
                new PacketGuiInteract(GuiInteraction.CONTAINER_TRACK_UPGRADES, helper.absolutePos(POS), MekanismContainer.UPGRADE_WINDOW).handle(context());
            });
            List<ISyncableData> tracked = trackedData();
            for (int i = before; i < tracked.size(); i++) {
                ISyncableData data = tracked.get(i);
                PropertyData property = data.getPropertyData(access(), (short) i, ISyncableData.DirtyType.DIRTY);
                sent.add(entry(i, data, ISyncableData.DirtyType.DIRTY, property));
            }
        }

        void closeUpgradeWindow() {
            packet(() -> {
                new PacketGuiInteract(GuiInteraction.CONTAINER_STOP_TRACKING, helper.absolutePos(POS), MekanismContainer.UPGRADE_WINDOW).handle(context());
                new PacketWindowSelect(null).handle(context());
            });
        }

        private void packet(Runnable handler) {
            AbstractContainerMenu old = player.containerMenu;
            player.containerMenu = menu;
            try {
                handler.run();
            } finally {
                player.containerMenu = old;
            }
        }

        private IPayloadContext context() {
            Player who = player;
            return (IPayloadContext) java.lang.reflect.Proxy.newProxyInstance(IPayloadContext.class.getClassLoader(), new Class<?>[]{IPayloadContext.class},
                  (proxy, method, args) -> {
                      if (method.getName().equals("player")) {
                          return who;
                      }
                      throw new UnsupportedOperationException(method.getName());
                  });
        }

        @SuppressWarnings("unchecked")
        private List<ISyncableData> trackedData() {
            try {
                Field field = MekanismContainer.class.getDeclaredField("trackedData");
                field.setAccessible(true);
                return (List<ISyncableData>) field.get(menu);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        /// Troca cada dado do menu por um espião durante a chamada e devolve os originais depois.
        private void withSpies(Runnable call) {
            List<ISyncableData> tracked = trackedData();
            List<ISyncableData> originals = new ArrayList<>(tracked);
            for (int i = 0; i < originals.size(); i++) {
                tracked.set(i, new Spy(originals.get(i), this));
            }
            try {
                call.run();
            } catch (RuntimeException e) {
                error = e.toString();
            } finally {
                for (int i = 0; i < originals.size(); i++) {
                    tracked.set(i, originals.get(i));
                }
            }
        }

        // ---- a gravação

        void log(short property, ISyncableData data, ISyncableData.DirtyType dirtyType, PropertyData value) {
            sink.add(entry(property, data, dirtyType, value));
        }

        private static String entry(int index, ISyncableData data, ISyncableData.DirtyType dirtyType, PropertyData value) {
            String name = data.getClass().getName();
            return "[" + index + ",\"" + name.substring(name.lastIndexOf('.') + 1) + "\",\"" + dirtyType + "\"," + valueJson(value) + "]";
        }

        private static String valueJson(PropertyData data) {
            try {
                Field field = data.getClass().getDeclaredField("value");
                field.setAccessible(true);
                Object value = field.get(data);
                if (value instanceof byte[] bytes) {
                    StringBuilder hex = new StringBuilder("\"");
                    for (byte b : bytes) {
                        hex.append(String.format("%02x", b));
                    }
                    return hex.append('"').toString();
                }
                if (value instanceof LargeResourceStack<?> stack) {
                    String id = stack.resource() instanceof RegisteredResource<?> registered && !stack.resource().isEmpty()
                          ? ((Holder<?>) registered.typeHolder()).getRegisteredName() : "";
                    return "{\"resource\":\"" + id + "\",\"amount\":" + stack.amount() + "}";
                }
                return String.valueOf(value);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private String marksJson() {
            StringBuilder out = new StringBuilder("[");
            for (int i = 0; i < marks.size(); i++) {
                out.append(i == 0 ? "" : ",").append('"').append(marks.get(i)).append('"');
            }
            marks.clear();
            return out.append(']').toString();
        }

        /// Lê por reflexão o deslocamento do recheck de erros, que o tile sorteia: a máquina de receita só reavalia os erros no tick
        /// em que `game_time % 100` bate com ele.
        private int recheckOffset() {
            if (recheck == null && tile() instanceof TileEntityRecipeMachine<?, ?> machine) {
                try {
                    Field field = TileEntityRecipeMachine.class.getDeclaredField("recheckAllRecipeErrors");
                    field.setAccessible(true);
                    recheck = (BooleanSupplier) field.get(machine);
                    for (Field captured : recheck.getClass().getDeclaredFields()) {
                        if (captured.getType() == int.class) {
                            captured.setAccessible(true);
                            recheckOffset = captured.getInt(recheck);
                        }
                    }
                } catch (ReflectiveOperationException e) {
                    throw new IllegalStateException(e);
                }
            }
            return recheckOffset;
        }

        private double heat(TileEntityHeatGenerator generator) {
            try {
                Field field = TileEntityHeatGenerator.class.getDeclaredField("heatCapacitor");
                field.setAccessible(true);
                return ((mekanism.common.capabilities.heat.BasicHeatCapacitor) field.get(generator)).getHeat();
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        /// O que a regra do gerador pede do mundo e o calor guardado: o ar do bloco e de cada face, a lava ao redor e o limite de troca
        /// de calor (as mesmas chaves do rastro `heat_generator_*`).
        private void heatExtras(StringBuilder line, TileEntityHeatGenerator generator) {
            line.append(",\"heat\":").append(heat(generator))
                  .append(",\"epsilon\":").append(mekanism.api.heat.HeatAPI.EPSILON)
                  .append(",\"ambient\":").append(mekanism.api.heat.HeatAPI.getAmbientTemp(helper.getLevel(), generator.getBlockPos()))
                  .append(",\"ambient_faces\":[");
            int lava = 0;
            for (Direction side : Direction.values()) {
                line.append(side.ordinal() == 0 ? "" : ",").append(generator.getAmbientTemperature(side));
                if (helper.getLevel().getFluidState(generator.getBlockPos().relative(side)).is(net.minecraft.tags.FluidTags.LAVA)) {
                    lava++;
                }
            }
            line.append("],\"lava_faces\":").append(lava).append(",\"produced\":").append(generator.getProductionRate())
                  .append(",\"transfer_loss\":").append(generator.getLastTransferLoss())
                  .append(",\"environment_loss\":").append(generator.getLastEnvironmentLoss());
        }

        /// `ServerTickEvent.Post`: o `broadcastChanges` que o `ServerPlayer.tick` do tick seguinte faria, e a linha do tick.
        void record() {
            if (helper == null) {
                return;
            }
            long gameTime = helper.getLevel().getGameTime();
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++).append(",\"game_time\":").append(gameTime);
            if (tick == 1 || reloaded) {
                line.append(",\"recheck_offset\":").append(recheckOffset());
                reloaded = false;
            }
            if (menu != null) {
                // O que `open` mandou já está em `sent`; o laço do broadcast só traz o que sujou depois.
                withSpies(menu::broadcastChanges);
            }
            line.append(",\"marks\":").append(marksJson());
            if (tile() instanceof TileEntityHeatGenerator generator) {
                heatExtras(line, generator);
            }
            if (menu != null) {
                String window = menu.getSelectedWindow(player.getUUID()) == null ? "none" : "upgrade";
                line.append(",\"window\":\"").append(window).append("\",\"tracked\":").append(trackedData().size());
            }
            line.append(",\"sent\":[").append(String.join(",", sent)).append("],\"dirty\":[").append(String.join(",", dirty)).append(']');
            if (error != null) {
                line.append(",\"error\":\"").append(error.replace("\"", "'")).append('"');
                error = null;
            }
            sent.clear();
            dirty.clear();
            lines.add(line.append('}').toString());
        }

        void write() {
            helper = null;
            try {
                Path file = Path.of("trace", name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }
}
