package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Supplier;
import mekanism.api.MekanismRegistries;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.inventory.IInventorySlot;
import mekanism.api.upgrade.IUpgradeHelper;
import mekanism.api.upgrade.Upgrade;
import mekanism.api.upgrade.UpgradeIds;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.inventory.container.MekanismContainer;
import mekanism.common.inventory.container.SelectedWindowData;
import mekanism.common.inventory.container.SelectedWindowData.WindowType;
import mekanism.common.inventory.container.tile.MekanismTileContainer;
import mekanism.common.network.to_server.PacketGuiInteract;
import mekanism.common.network.to_server.PacketGuiInteract.GuiInteraction;
import mekanism.common.network.to_server.PacketRemoveUpgrade;
import mekanism.common.network.to_server.PacketWindowSelect;
import mekanism.common.registration.impl.ContainerTypeRegistryObject;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismContainerTypes;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.component.TileComponentUpgrade;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import mekanism.common.tile.prefab.TileEntityProgressMachine;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.registries.GeneratorsContainerTypes;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.network.handling.IPayloadContext;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o que acontece quando o jogador salva e sai (`Save and Quit`) com a janela de upgrades de uma máquina aberta, e abre o mundo de novo.
/// `ServerGamePacketListenerImpl.removePlayerFromWorld` salva o jogador (`PlayerList.remove`) e tira a entidade (`Player.remove`, que fecha o menu com
/// `AbstractContainerMenu.removed`); o chunk é gravado e carregado depois (`BlockEntity.saveWithFullMetadata` e `loadStatic`). Aqui: o menu e a janela
/// ficam abertos com upgrades no slot da janela (instalando, os 20 ticks de `TileComponentUpgrade.UPGRADE_TICKS_REQUIRED`), a máquina andando com
/// energia, entrada e progresso, e `quit` faz `menu.removed(player)` e recarrega o tile. `upgrade_quit_<máquina>.jsonl`, um tick por linha.
@ForEachTest(groups = "trace")
public class UpgradeQuitTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final BlockPos FLOOR = new BlockPos(3, 0, 1);
    private static final BlockPos SPOT = new BlockPos(3, 1, 1);
    private static final int REFILL_TICKS = 3;

    private record Spec(String name, Supplier<ItemLike> item, Supplier<ContainerTypeRegistryObject<?>> menu) {
    }

    private static final Spec CHAMBER = new Spec("enrichment_chamber", () -> MekanismBlocks.ENRICHMENT_CHAMBER.asItem(),
          () -> MekanismContainerTypes.ENRICHMENT_CHAMBER);
    private static final Spec INFUSER = new Spec("metallurgic_infuser", () -> MekanismBlocks.METALLURGIC_INFUSER.asItem(),
          () -> MekanismContainerTypes.METALLURGIC_INFUSER);
    private static final Spec HEAT = new Spec("heat_generator", () -> GeneratorsBlocks.HEAT_GENERATOR.asItem(),
          () -> GeneratorsContainerTypes.HEAT_GENERATOR);

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(6, 3, 3).fill(3, 0, 1, 3, 0, 1, Blocks.STONE.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_upgrade_quit")
    @TestHolder(description = "Records the Enrichment Chamber saved and loaded with the upgrade window open: upgrades installing from the slot, then the machine in the middle of a recipe with upgrades in the slot.")
    public static void chamberQuit(final DynamicTest test) {
        template(test);
        Scene scene = new Scene(CHAMBER);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper))
              .thenExecuteAfter(5, () -> scene.apply("item clay 8"))
              .thenExecuteAfter(30, () -> scene.apply("open"))
              .thenExecuteAfter(5, () -> scene.apply("window"))
              .thenExecuteAfter(5, () -> scene.apply("slot speed 4"))
              .thenExecuteAfter(8, () -> scene.apply("quit"))
              .thenExecuteAfter(40, () -> scene.apply("open"))
              .thenExecuteAfter(5, () -> scene.apply("window"))
              .thenExecuteAfter(5, () -> scene.apply("slot energy 3"))
              .thenExecuteAfter(19, () -> scene.apply("quit"))
              .thenExecuteAfter(40, () -> scene.apply("open"))
              .thenExecuteAfter(5, () -> scene.apply("window"))
              .thenExecuteAfter(5, () -> scene.apply("slot speed 2"))
              .thenExecuteAfter(21, () -> scene.apply("quit"))
              .thenExecuteAfter(60, () -> scene.apply("open"))
              .thenExecuteAfter(5, () -> scene.apply("window"))
              .thenExecuteAfter(5, () -> scene.apply("slot energy 40"))
              .thenExecuteAfter(30, () -> scene.apply("quit"))
              .thenExecuteAfter(60, () -> scene.apply("open"))
              .thenExecuteAfter(5, () -> scene.apply("window"))
              .thenExecuteAfter(5, () -> scene.apply("remove speed"))
              .thenExecuteAfter(10, () -> scene.apply("quit"))
              .thenExecuteAfter(60, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_upgrade_quit")
    @TestHolder(description = "Records the Metallurgic Infuser saved and loaded with the upgrade window open, with the chemical tank and the recipe in progress.")
    public static void infuserQuit(final DynamicTest test) {
        template(test);
        Scene scene = new Scene(INFUSER);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper))
              .thenExecuteAfter(5, () -> scene.apply("item dirt 6, tank bio 500"))
              .thenExecuteAfter(40, () -> scene.apply("open"))
              .thenExecuteAfter(5, () -> scene.apply("window"))
              .thenExecuteAfter(5, () -> scene.apply("slot speed 3"))
              .thenExecuteAfter(10, () -> scene.apply("quit"))
              .thenExecuteAfter(50, () -> scene.apply("open"))
              .thenExecuteAfter(5, () -> scene.apply("window"))
              .thenExecuteAfter(5, () -> scene.apply("slot energy 5"))
              .thenExecuteAfter(15, () -> scene.apply("quit"))
              .thenExecuteAfter(60, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_upgrade_quit")
    @TestHolder(description = "Records the Heat Generator saved and loaded with the upgrade window open, burning fuel with a muffling upgrade installing from the slot.")
    public static void heatQuit(final DynamicTest test) {
        template(test);
        Scene scene = new Scene(HEAT);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper))
              .thenExecuteAfter(5, () -> scene.apply("item coal 8"))
              .thenExecuteAfter(30, () -> scene.apply("open"))
              .thenExecuteAfter(5, () -> scene.apply("window"))
              .thenExecuteAfter(5, () -> scene.apply("slot muffling 3"))
              .thenExecuteAfter(9, () -> scene.apply("quit"))
              .thenExecuteAfter(50, () -> scene.apply("open"))
              .thenExecuteAfter(5, () -> scene.apply("window"))
              .thenExecuteAfter(5, () -> scene.apply("slot muffling 1"))
              .thenExecuteAfter(25, () -> scene.apply("quit"))
              .thenExecuteAfter(60, scene::write)
              .thenSucceed());
    }

    private static final class Scene {
        private final Spec spec;
        private final String name;
        private final List<String> lines = new ArrayList<>();
        private final List<String> marks = new ArrayList<>();
        private MekGameTestHelper helper;
        private Player player;
        private MekanismTileContainer<TileEntityMekanism> menu;
        private boolean windowSelected;
        private int tick;

        Scene(Spec spec) {
            this.spec = spec;
            this.name = "upgrade_quit_" + spec.name;
        }

        void begin(MekGameTestHelper helper) {
            this.helper = helper;
            player = helper.makeMockPlayerLookingAt(FLOOR, Direction.UP);
            helper.useOn(FLOOR, new ItemStack(spec.item.get()), player, Direction.UP);
        }

        TileEntityMekanism tile() {
            return (TileEntityMekanism) helper.getLevel().getBlockEntity(helper.absolutePos(SPOT));
        }

        private RegistryAccess access() {
            return helper.getLevel().registryAccess();
        }

        private Holder<Upgrade> upgrade(ResourceKey<Upgrade> key) {
            return access().lookupOrThrow(MekanismRegistries.Keys.UPGRADES).getOrThrow(key);
        }

        private static ResourceKey<Upgrade> key(String id) {
            return switch (id) {
                case "speed" -> UpgradeIds.SPEED;
                case "energy" -> UpgradeIds.ENERGY;
                case "muffling" -> UpgradeIds.MUFFLING;
                default -> throw new IllegalStateException(id);
            };
        }

        /// Uma marca: o que o cenário fez depois daquele tick. As partes separadas por vírgula acontecem juntas.
        void apply(String mark) {
            marks.add(mark);
            for (String action : mark.split(", ")) {
                String[] words = action.split(" ");
                switch (words[0]) {
                    case "item" -> {
                        Item item = switch (words[1]) {
                            case "clay" -> Items.CLAY;
                            case "dirt" -> Items.DIRT;
                            case "coal" -> Items.COAL;
                            default -> throw new IllegalStateException(action);
                        };
                        tile().getInventorySlots().get(inputIndex()).setContents(ItemResource.of(item), Integer.parseInt(words[2]), null);
                    }
                    case "tank" -> {
                        Holder<Chemical> holder = access().getOrThrow(ChemicalIds.BIO);
                        ((TileEntityMetallurgicInfuser) tile()).infusionTank.setContents(ChemicalResource.of(holder), Long.parseLong(words[2]), null);
                    }
                    case "open" -> {
                        menu = new MekanismTileContainer<>(spec.menu.get(), 100, player.getInventory(), tile());
                        windowSelected = false;
                    }
                    case "window" -> {
                        packet(() -> {
                            new PacketWindowSelect(new SelectedWindowData(WindowType.UPGRADE)).handle(context());
                            new PacketGuiInteract(GuiInteraction.CONTAINER_TRACK_UPGRADES, helper.absolutePos(SPOT), MekanismContainer.UPGRADE_WINDOW).handle(context());
                        });
                        windowSelected = true;
                    }
                    case "slot" -> {
                        menu.setCarried(IUpgradeHelper.INSTANCE.asResource(upgrade(key(words[1]))).toStack(Integer.parseInt(words[2])));
                        menu.clicked(menu.getUpgradeSlot().index, 0, ContainerInput.PICKUP, player);
                        //O que o slot não aceitou fica no cursor, que o fechamento do menu devolve ao jogador.
                        marks.add("carried " + (menu.getCarried().isEmpty() ? "nothing" : BuiltInRegistries.ITEM.getKey(menu.getCarried().getItem()).getPath() + " " + menu.getCarried().getCount()));
                    }
                    case "remove" -> packet(() -> new PacketRemoveUpgrade(helper.absolutePos(SPOT), upgrade(key(words[1])), true).handle(context()));
                    case "quit" -> quit();
                    default -> throw new IllegalStateException(action);
                }
            }
        }

        private int inputIndex() {
            return spec == INFUSER ? 1 : 0;
        }

        /// `Save and Quit`: o jogador sai (`Player.remove` fecha o menu), o chunk grava o tile e o mundo reaberto o lê (`loadStatic`). O
        /// tile é gravado com o menu ainda aberto (`PlayerList.remove` salva o jogador antes de tirar a entidade), e o menu fecha depois.
        private void quit() {
            TileEntityMekanism old = tile();
            BlockPos pos = helper.absolutePos(SPOT);
            CompoundTag tag = old.saveWithFullMetadata(access());
            if (menu != null) {
                menu.removed(player);
                menu = null;
            }
            windowSelected = false;
            BlockEntity loaded = BlockEntity.loadStatic(pos, helper.getLevel().getBlockState(pos), tag, access());
            if (loaded == null || loaded == old) {
                throw new IllegalStateException("loadStatic gave " + loaded);
            }
            LevelChunk chunk = helper.getLevel().getChunkAt(pos);
            old.onChunkUnloaded();
            chunk.removeBlockEntity(pos);
            chunk.addAndRegisterBlockEntity(loaded);
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

        private static int upgradeTicks(TileComponentUpgrade component) {
            try {
                Field field = TileComponentUpgrade.class.getDeclaredField("upgradeTicks");
                field.setAccessible(true);
                return field.getInt(component);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private static void slot(StringBuilder line, ItemStack stack) {
            line.append('"').append(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath()).append("\",").append(stack.getCount());
        }

        private void upgradeSlot(StringBuilder line, IInventorySlot slot) {
            ItemStack stack = slot.asItemStack();
            Holder<Upgrade> type = stack.isEmpty() ? null : stack.get(IUpgradeHelper.INSTANCE.dataComponent());
            line.append('"').append(type == null ? "" : type.getRegisteredName().substring("mekanism:".length())).append("\",").append(stack.getCount());
        }

        void record() {
            if (helper == null || tile() == null) {
                return;
            }
            TileEntityMekanism tile = tile();
            TileComponentUpgrade component = tile.getComponent();
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++).append(",\"marks\":[");
            for (int i = 0; i < marks.size(); i++) {
                line.append(i == 0 ? "" : ",").append('"').append(marks.get(i)).append('"');
            }
            marks.clear();
            line.append("],\"menu\":").append(menu != null).append(",\"window\":").append(windowSelected)
                  .append(",\"energy\":").append(tile.getEnergyContainer().getAmountAsLong())
                  .append(",\"capacity\":").append(tile.getEnergyContainer().getCapacityAsLong());
            if (tile instanceof TileEntityProgressMachine<?> machine) {
                line.append(",\"progress\":").append(machine.getRecipeProgress())
                      .append(",\"required\":").append(machine.getTicksRequired())
                      .append(",\"active\":").append(machine.getActive())
                      .append(",\"input\":[");
                slot(line, machine.getInventorySlots().get(inputIndex()).asItemStack());
                line.append("],\"output\":[");
                slot(line, machine.getInventorySlots().get(inputIndex() + 1).asItemStack());
                line.append(']');
            }
            if (tile instanceof TileEntityMetallurgicInfuser infuser) {
                line.append(",\"tank\":").append(infuser.infusionTank.amountAsLong());
            }
            if (tile instanceof TileEntityHeatGenerator heat) {
                line.append(",\"fuel\":[");
                slot(line, heat.getInventorySlots().get(0).asItemStack());
                line.append("],\"lava\":").append(heat.lavaTank.amountAsLong());
            }
            line.append(",\"speed\":").append(component.getUpgrades(upgrade(UpgradeIds.SPEED)))
                  .append(",\"energy_upgrades\":").append(component.getUpgrades(upgrade(UpgradeIds.ENERGY)))
                  .append(",\"muffling\":").append(component.getUpgrades(upgrade(UpgradeIds.MUFFLING)))
                  .append(",\"upgrade_ticks\":").append(upgradeTicks(component))
                  .append(",\"upgrade_in\":[");
            upgradeSlot(line, component.getUpgradeSlot());
            line.append("],\"upgrade_out\":[");
            upgradeSlot(line, component.getUpgradeOutputSlot());
            line.append("]}");
            lines.add(line.toString());
            if (tile.getEnergyContainer() instanceof MachineEnergyContainer<?> energy) {
                energy.setEnergy(REFILL_TICKS * (long) energy.getEnergyPerTick(), null);
            }
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
