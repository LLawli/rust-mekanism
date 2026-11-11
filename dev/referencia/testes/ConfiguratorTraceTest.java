package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import mekanism.api.MekanismItemAbilities;
import mekanism.api.RelativeSide;
import mekanism.api.radial.RadialData;
import mekanism.api.security.IItemSecurityUtils;
import mekanism.api.text.EnumColor;
import mekanism.common.block.attribute.Attribute;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.component.ConfigurationData;
import mekanism.common.component.component.AttachedEjector;
import mekanism.common.component.component.AttachedSideConfig;
import mekanism.common.item.ItemConfigurator;
import mekanism.common.item.ItemConfigurator.ConfiguratorMode;
import mekanism.common.item.interfaces.IModeItem;
import mekanism.common.item.interfaces.IModeItem.DisplayChange;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismDataComponents;
import mekanism.common.registries.MekanismItems;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.component.TileComponentEjector;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.tile.interfaces.IRedstoneControl.RedstoneControl;
import mekanism.common.tile.interfaces.ISideConfiguration;
import mekanism.common.util.ItemAccessUtils;
import mekanism.generators.common.registries.GeneratorsBlocks;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponentPatch;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.common.ClientboundCustomPayloadPacket;
import net.minecraft.network.protocol.game.ClientboundSystemChatPacket;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.ItemAbility;
import net.neoforged.neoforge.transfer.access.ItemAccess;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.GameTestPlayer;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Frente `configurator`: grava o `ItemConfigurator` e o `ItemConfigurationCard` pelo caminho de verdade do servidor (`ServerPlayerGameMode.useItemOn`,
/// `useItem` e o handler de `PacketModeChange`), com um jogador de teste (`GameTestPlayer`, um `ServerPlayer` com conexão simulada, em sobrevivência) cujos
/// pacotes enviados (mensagens de overlay, abertura de menu, payloads) o teste lê da fila do canal. Um `runGameTestServer`, os arquivos
/// `configurator_*.jsonl`.
@ForEachTest(groups = "trace")
public class ConfiguratorTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final String COMMIT = "b2d04e166";
    private static final String DATE = "2026-11-11";
    private static final BlockPos P1 = new BlockPos(1, 1, 1);
    private static final BlockPos P2 = new BlockPos(3, 1, 1);
    private static final String BATCH = "trace_configurator";
    private static final int ENERGY = 12345;

    private record Spec(String name, ItemLike item, BlockState state, int input, int output, int infusion) {
    }

    private static Spec chamber() {
        return new Spec("enrichment_chamber", MekanismBlocks.ENRICHMENT_CHAMBER.asItem(), MekanismBlocks.ENRICHMENT_CHAMBER.defaultState(), 0, 1, -1);
    }

    private static Spec crusher() {
        return new Spec("crusher", MekanismBlocks.CRUSHER.asItem(), MekanismBlocks.CRUSHER.defaultState(), 0, 1, -1);
    }

    private static Spec infuser() {
        return new Spec("metallurgic_infuser", MekanismBlocks.METALLURGIC_INFUSER.asItem(), MekanismBlocks.METALLURGIC_INFUSER.defaultState(), 1, 2, 0);
    }

    private static Spec heat() {
        return new Spec("heat_generator", GeneratorsBlocks.HEAT_GENERATOR.asItem(), GeneratorsBlocks.HEAT_GENERATOR.defaultState(), 0, -1, -1);
    }

    private static List<Spec> configurable() {
        return List.of(chamber(), infuser(), crusher());
    }

    private static List<Spec> allMachines() {
        return List.of(chamber(), infuser(), crusher(), heat());
    }

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(5, 3, 3).fill(0, 0, 1, 4, 0, 1, Blocks.STONE.defaultBlockState()));
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Os testes
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the eight ConfiguratorModes: names, colors, texts, tooltips, adjust, abilities and doesSneakBypassUse.")
    public static void modes(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> modesAll(new Env(helper)))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records changeMode the way PacketModeChange.handle calls it, from every mode, with every shift and DisplayChange.")
    public static void changeMode(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> changeModeAll(new Env(helper)))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the real click of the Configurator on the Enrichment Chamber through ServerPlayerGameMode.useItemOn.")
    public static void clickChamber(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> clickMachine(new Env(helper), chamber()))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the real click of the Configurator on the Metallurgic Infuser through ServerPlayerGameMode.useItemOn.")
    public static void clickInfuser(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> clickMachine(new Env(helper), infuser()))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the real click of the Configurator on the Crusher through ServerPlayerGameMode.useItemOn.")
    public static void clickCrusher(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> clickMachine(new Env(helper), crusher()))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the real click of the Configurator on the Heat Generator through ServerPlayerGameMode.useItemOn.")
    public static void clickHeat(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> clickMachine(new Env(helper), heat()))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the real click of the Configurator on stone, a chest and air through ServerPlayerGameMode.useItemOn.")
    public static void clickOthers(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> clickOthers(new Env(helper)))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records twelve sneaking Configurator clicks in a row on the same face of the Enrichment Chamber and the Metallurgic Infuser.")
    public static void clickSequence(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> clickSequenceAll(new Env(helper)))
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the configuration card through ServerPlayerGameMode.useItemOn and useItem.")
    public static void card(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> cardAll(new Env(helper)))
              .thenSucceed());
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Escrita
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final class Out {

        private final String name;
        private final List<String> lines = new ArrayList<>();

        Out(String name, String description, String fields) {
            this.name = name;
            JsonObject meta = new JsonObject();
            meta.addProperty("commit", COMMIT);
            meta.addProperty("date", DATE);
            meta.addProperty("file", name + ".jsonl");
            meta.addProperty("description", description);
            meta.addProperty("fields", fields);
            JsonObject line = new JsonObject();
            line.add("meta", meta);
            lines.add(line.toString());
        }

        void add(JsonObject line) {
            lines.add(line.toString());
        }

        void write() {
            try {
                Path file = Path.of("trace", name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Ambiente
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final class Env {

        final MekGameTestHelper helper;
        final GameTestPlayer player;

        Env(MekGameTestHelper helper) {
            this.helper = helper;
            this.player = (GameTestPlayer) helper.makeMockPlayerLookingAt(P1.below(), Direction.UP);
            this.player.gameMode.changeGameModeForPlayer(GameType.SURVIVAL);
            this.player.preventItemPickup();
        }

        ServerLevel level() {
            return helper.getLevel();
        }

        BlockPos abs(BlockPos rel) {
            return helper.absolutePos(rel);
        }

        TileEntityMekanism tile(BlockPos rel) {
            return level().getBlockEntity(abs(rel)) instanceof TileEntityMekanism tile ? tile : null;
        }

        void clear(BlockPos rel) {
            helper.setBlock(rel, Blocks.AIR.defaultBlockState());
        }

        /// A máquina posta por um jogador (`useOn` com o item), voltada para `facing`, com a energia 12345 e os slots cheios.
        TileEntityMekanism place(Spec spec, BlockPos rel, Direction facing, boolean fill) {
            clear(rel);
            player.setShiftKeyDown(false);
            player.setYRot(facing.getOpposite().toYRot());
            helper.useOn(rel.below(), new ItemStack(spec.item), player, Direction.UP);
            TileEntityMekanism tile = tile(rel);
            if (tile == null || tile.getDirection() != facing) {
                throw new IllegalStateException("placement by player gave " + (tile == null ? null : tile.getDirection()) + " instead of " + facing);
            }
            if (tile.getEnergyContainer() instanceof MachineEnergyContainer<?> energy) {
                energy.setMaxEnergy(1_000_000);
                energy.setEnergy(0, null);
            }
            if (fill) {
                fill(tile, spec);
            }
            return tile;
        }

        void fill(TileEntityMekanism tile, Spec spec) {
            if (tile.getEnergyContainer() != null) {
                tile.getEnergyContainer().setEnergy(ENERGY, null);
            }
            if (spec.infusion >= 0) {
                tile.getInventorySlots().get(spec.infusion).setContents(ItemResource.of(Items.STICK), 3, null);
            }
            if (spec.input >= 0) {
                tile.getInventorySlots().get(spec.input).setContents(ItemResource.of(Items.STICK), spec.infusion >= 0 ? 4 : 5, null);
            }
            if (spec.output >= 0) {
                tile.getInventorySlots().get(spec.output).setContents(ItemResource.of(Items.CLAY_BALL), 7, null);
            }
        }

        void hand(ItemStack stack) {
            player.setItemInHand(InteractionHand.MAIN_HAND, stack);
        }

        BlockHitResult hit(BlockPos rel, Direction face) {
            BlockPos abs = abs(rel);
            return new BlockHitResult(Vec3.atCenterOf(abs).relative(face, 0.5), face, abs, false);
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // JSON
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    /// O componente em JSON. O `uuid` de um `hover_event` (o jogador de teste sorteia o dele) vira `[0,0,0,0]`, para o rastro ser igual a cada execução.
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

    private static JsonArray tooltip(Env e, ItemStack stack) {
        JsonArray lines = new JsonArray();
        for (Component component : stack.getTooltipLines(Item.TooltipContext.of(e.level()), null, TooltipFlag.NORMAL)) {
            lines.add(component(component));
        }
        return lines;
    }

    private static JsonArray strings(Iterable<String> values) {
        JsonArray array = new JsonArray();
        for (String value : values) {
            array.add(value);
        }
        return array;
    }

    private static String dt(DataType type) {
        return type.getSerializedName();
    }

    private static JsonElement sorted(JsonElement element) {
        if (element.isJsonObject()) {
            JsonObject in = element.getAsJsonObject();
            List<String> keys = new ArrayList<>(in.keySet());
            Collections.sort(keys);
            JsonObject out = new JsonObject();
            for (String key : keys) {
                out.add(key, sorted(in.get(key)));
            }
            return out;
        } else if (element.isJsonArray()) {
            JsonArray out = new JsonArray();
            for (JsonElement child : element.getAsJsonArray()) {
                out.add(sorted(child));
            }
            return out;
        }
        return element;
    }

    private static JsonElement nbtJson(Tag tag) {
        return tag == null ? JsonNull.INSTANCE : sorted(NbtOps.INSTANCE.convertTo(JsonOps.INSTANCE, tag));
    }

    private static JsonArray patchKeys(ItemStack stack) {
        List<String> keys = new ArrayList<>();
        DataComponentPatch.SplitResult split = stack.getComponentsPatch().split();
        for (DataComponentType<?> type : split.added().keySet()) {
            keys.add(String.valueOf(BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(type)));
        }
        for (DataComponentType<?> type : split.removed()) {
            keys.add("!" + BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(type));
        }
        Collections.sort(keys);
        return strings(keys);
    }

    @SuppressWarnings("unchecked")
    private static <T> JsonElement encode(ServerLevel level, com.mojang.serialization.Codec<T> codec, T value) {
        if (value == null) {
            return JsonNull.INSTANCE;
        }
        return codec.encodeStart(level.registryAccess().createSerializationContext(JsonOps.INSTANCE), value).getOrThrow(IllegalStateException::new);
    }

    /// A pilha: item, quantidade, as chaves do patch de componentes (sem o dono, que traz o UUID do jogador), o modo do Configurator e o dado do cartão.
    private static JsonObject stackJson(Env e, ItemStack stack) {
        JsonObject out = new JsonObject();
        if (stack.isEmpty()) {
            out.addProperty("item", "");
            out.addProperty("count", 0);
            return out;
        }
        out.addProperty("item", BuiltInRegistries.ITEM.getKey(stack.getItem()).toString());
        out.addProperty("count", stack.getCount());
        out.add("patch", patchKeys(stack));
        ConfiguratorMode mode = stack.get(MekanismDataComponents.CONFIGURATOR_MODE);
        out.addProperty("configurator_mode", mode == null ? null : mode.getSerializedName());
        ConfigurationData data = stack.get(MekanismDataComponents.CONFIGURATION_DATA);
        CompoundTag config = data == null ? null : data.configuration();
        out.addProperty("configuration_data", config == null ? null : config.toString());
        out.add("side_config", encode(e.level(), AttachedSideConfig.CODEC, stack.get(MekanismDataComponents.SIDE_CONFIG)));
        out.add("ejector", encode(e.level(), AttachedEjector.CODEC, stack.get(MekanismDataComponents.EJECTOR)));
        return out;
    }

    private static JsonObject configJson(ISideConfiguration machine) {
        JsonObject config = new JsonObject();
        for (TransmissionType type : machine.getConfig().getTransmissions()) {
            JsonArray sides = new JsonArray();
            ConfigInfo info = machine.getConfig().getConfig(type);
            for (RelativeSide side : RelativeSide.VALUES) {
                sides.add(dt(info.getDataType(side)));
            }
            config.add(type.getSerializedName(), sides);
        }
        return config;
    }

    private static JsonObject ejectingJson(ISideConfiguration machine) {
        JsonObject ejecting = new JsonObject();
        for (TransmissionType type : machine.getConfig().getTransmissions()) {
            ejecting.addProperty(type.getSerializedName(), machine.getConfig().getConfig(type).isEjecting());
        }
        return ejecting;
    }

    private static JsonObject ejectorJson(ISideConfiguration machine) {
        TileComponentEjector ejector = machine.getEjector();
        JsonObject out = new JsonObject();
        out.addProperty("strict_input", ejector.hasStrictInput());
        out.addProperty("output_color", ejector.getOutputColor() == null ? "" : ejector.getOutputColor().getSerializedName());
        JsonArray inputs = new JsonArray();
        for (RelativeSide side : RelativeSide.VALUES) {
            EnumColor color = ejector.getInputColor(side);
            inputs.add(color == null ? "" : color.getSerializedName());
        }
        out.add("input_colors", inputs);
        return out;
    }

    /// O que há em `rel`: o bloco, o tile, a direção, o modo de redstone, a configuração de lados, o ejetor, a energia e os slots de item.
    private static JsonObject snapshot(Env e, BlockPos rel) {
        JsonObject out = new JsonObject();
        BlockState state = e.level().getBlockState(e.abs(rel));
        out.addProperty("block", BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString());
        BlockEntity be = e.level().getBlockEntity(e.abs(rel));
        out.addProperty("tile", be == null ? null : be.getClass().getSimpleName());
        if (be instanceof TileEntityMekanism tile) {
            out.addProperty("facing", tile.getDirection().getSerializedName());
            out.addProperty("control_type", tile.getControlType().getSerializedName());
            if (tile instanceof ISideConfiguration side) {
                out.add("config", configJson(side));
                out.add("ejecting", ejectingJson(side));
                out.add("ejector", ejectorJson(side));
            }
            out.addProperty("energy", tile.getEnergyContainer() == null ? null : String.valueOf(tile.getEnergyContainer().getAmountAsLong()));
            JsonArray slots = new JsonArray();
            for (var slot : tile.getInventorySlots()) {
                ItemStack stack = slot.asItemStack();
                slots.add(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath() + "*" + stack.getCount());
            }
            out.add("slots", slots);
        }
        return out;
    }

    private static JsonObject result(InteractionResult result) {
        JsonObject out = new JsonObject();
        out.addProperty("class", result.getClass().getSimpleName());
        out.addProperty("consumes_action", result.consumesAction());
        if (result instanceof InteractionResult.Success success) {
            out.addProperty("swing_source", success.swingSource().name());
            out.addProperty("was_item_interaction", success.wasItemInteraction());
            out.addProperty("held_item_transformed", success.heldItemTransformedTo() != null);
        }
        return out;
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    private static List<Packet<?>> packets(GameTestPlayer player) {
        return (List) ((java.util.stream.Stream) player.getOutboundPackets((Class) Packet.class)).toList();
    }

    /// Os pacotes que o jogador recebeu desde a última chamada: as mensagens de overlay (JSON do componente), as de chat, os ids dos payloads personalizados.
    private static JsonObject drain(GameTestPlayer player) {
        JsonArray overlay = new JsonArray();
        JsonArray chat = new JsonArray();
        JsonArray payloads = new JsonArray();
        for (Packet<?> packet : packets(player)) {
            if (packet instanceof ClientboundSystemChatPacket system) {
                (system.overlay() ? overlay : chat).add(component(system.content()));
            } else if (packet instanceof ClientboundCustomPayloadPacket custom) {
                payloads.add(custom.payload().type().id().toString());
            }
        }
        player.clearOutboundPackets();
        JsonObject out = new JsonObject();
        out.add("overlay", overlay);
        out.add("chat", chat);
        out.add("payloads", payloads);
        return out;
    }

    /// O menu aberto do jogador (`containerMenu` diferente do inventário), ou nulo; fecha-o.
    private static JsonElement openMenu(GameTestPlayer player) {
        AbstractContainerMenu menu = player.containerMenu;
        if (menu == player.inventoryMenu) {
            return JsonNull.INSTANCE;
        }
        JsonObject out = new JsonObject();
        String type;
        try {
            type = String.valueOf(BuiltInRegistries.MENU.getKey(menu.getType()));
        } catch (RuntimeException ex) {
            type = "?";
        }
        out.addProperty("type", type);
        out.addProperty("class", menu.getClass().getSimpleName());
        player.closeContainer();
        return out;
    }

    private static JsonArray drops(Env e, BlockPos rel) {
        List<JsonObject> found = new ArrayList<>();
        for (ItemEntity entity : e.level().getEntitiesOfClass(ItemEntity.class, new AABB(e.abs(rel)).inflate(3))) {
            found.add(stackJson(e, entity.getItem()));
            entity.discard();
        }
        found.sort((a, b) -> a.toString().compareTo(b.toString()));
        JsonArray array = new JsonArray();
        found.forEach(array::add);
        return array;
    }

    private static JsonArray inventory(Env e) {
        JsonArray array = new JsonArray();
        for (int i = 0; i < e.player.getInventory().getContainerSize(); i++) {
            ItemStack stack = e.player.getInventory().getItem(i);
            if (!stack.isEmpty()) {
                JsonObject entry = stackJson(e, stack);
                entry.addProperty("slot", i);
                array.add(entry);
            }
        }
        return array;
    }

    private static JsonArray marks(String... marks) {
        JsonArray array = new JsonArray();
        for (String mark : marks) {
            array.add(mark);
        }
        return array;
    }

    private static ItemStack configurator(ConfiguratorMode mode) {
        ItemStack stack = new ItemStack(MekanismItems.CONFIGURATOR.asItem());
        stack.set(MekanismDataComponents.CONFIGURATOR_MODE, mode);
        return stack;
    }

    private static final List<ItemAbility> ABILITIES = List.of(MekanismItemAbilities.WRENCH_CONFIGURE, MekanismItemAbilities.WRENCH_CONFIGURE_CHEMICALS,
          MekanismItemAbilities.WRENCH_CONFIGURE_ENERGY, MekanismItemAbilities.WRENCH_CONFIGURE_FLUIDS, MekanismItemAbilities.WRENCH_CONFIGURE_HEAT,
          MekanismItemAbilities.WRENCH_CONFIGURE_ITEMS, MekanismItemAbilities.WRENCH_DISMANTLE, MekanismItemAbilities.WRENCH_EMPTY, MekanismItemAbilities.WRENCH_ROTATE);

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Arquivo 1: configurator_modes
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static void modesAll(Env e) {
        Out out = new Out("configurator_modes",
              "The eight ConfiguratorModes and what the Configurator stack says in each of them.",
              "case=mode lines: mode (getSerializedName), ordinal, transmission (TransmissionType serialized name or null), color {name (EnumColor serialized), rgb [r,g,b] (getRgbCode), packed (getPackedColor)}, "
                    + "is_configurating, text_component (getTextComponent, ComponentSerialization.CODEC JSON), slice_name (sliceName), add_to_tooltip (the lines ConfiguratorMode.addToTooltip emits, Component JSON), "
                    + "item_tooltip (ItemStack.getTooltipLines of a Configurator stack holding the mode, TooltipFlag.NORMAL, no player: the name line and the appenders), "
                    + "scroll_text (ItemConfigurator.getScrollTextComponent), icon (IRadialMode.icon), hud_strings (ItemConfigurator.addHUDStrings in the main hand, Component JSON), "
                    + "is_mode_item and is_mode_item_without_radial (IModeItem.isModeItem with allowRadial true and false), adjust {shift -9..9 as string key: serialized name of IIncrementalEnum.adjust(shift)}, "
                    + "abilities {WRENCH_CONFIGURE, WRENCH_CONFIGURE_CHEMICALS, WRENCH_CONFIGURE_ENERGY, WRENCH_CONFIGURE_FLUIDS, WRENCH_CONFIGURE_HEAT, WRENCH_CONFIGURE_ITEMS, WRENCH_DISMANTLE, WRENCH_EMPTY, WRENCH_ROTATE: "
                    + "ItemStack.canPerformAction on a Configurator stack in the mode}, sneak_bypass_use (ItemStack.doesSneakBypassUse), patch (component patch keys of the stack; the default mode ITEMS leaves the patch empty). "
                    + "case=radial: the RadialData of the item: id, modes (count), default_mode, network_representation and index (per mode, in order), same_data_every_call. "
                    + "case=fresh: a new Configurator stack with no component set (the item prototype brings CONFIGURATOR_MODE=configurate_items): mode_component, item_tooltip, patch. "
                    + "case=removed: the same stack after remove(CONFIGURATOR_MODE): mode_component (null), item_tooltip, patch, get_mode (the mode ItemConfigurator.getMode answers: the default), abilities, sneak_bypass_use.");
        ItemConfigurator item = MekanismItems.CONFIGURATOR.get();
        BlockPos pos = e.abs(P1);
        for (ConfiguratorMode mode : ConfiguratorMode.VALUES) {
            ItemStack stack = configurator(mode);
            JsonObject line = new JsonObject();
            line.addProperty("case", "mode");
            line.addProperty("mode", mode.getSerializedName());
            line.addProperty("ordinal", mode.ordinal());
            line.addProperty("transmission", mode.getTransmission() == null ? null : mode.getTransmission().getSerializedName());
            JsonObject color = new JsonObject();
            color.addProperty("name", mode.color().getSerializedName());
            JsonArray rgb = new JsonArray();
            for (int channel : mode.color().getRgbCode()) {
                rgb.add(channel);
            }
            color.add("rgb", rgb);
            color.addProperty("packed", mode.color().getPackedColor());
            line.add("color", color);
            line.addProperty("is_configurating", mode.isConfigurating());
            line.add("text_component", component(mode.getTextComponent()));
            line.add("slice_name", component(mode.sliceName()));
            JsonArray added = new JsonArray();
            mode.addToTooltip(Item.TooltipContext.of(e.level()), c -> added.add(component(c)), TooltipFlag.NORMAL, stack);
            line.add("add_to_tooltip", added);
            line.add("item_tooltip", tooltip(e, stack));
            line.add("scroll_text", component(item.getScrollTextComponent(stack)));
            line.addProperty("icon", mode.icon().toString());
            List<Component> hud = new ArrayList<>();
            item.addHUDStrings(hud, e.player, stack, EquipmentSlot.MAINHAND);
            JsonArray hudStrings = new JsonArray();
            for (Component c : hud) {
                hudStrings.add(component(c));
            }
            line.add("hud_strings", hudStrings);
            line.addProperty("is_mode_item", IModeItem.isModeItem(stack, EquipmentSlot.MAINHAND));
            line.addProperty("is_mode_item_without_radial", IModeItem.isModeItem(stack, EquipmentSlot.MAINHAND, false));
            JsonObject adjust = new JsonObject();
            for (int shift = -9; shift <= 9; shift++) {
                adjust.addProperty(String.valueOf(shift), mode.adjust(shift).getSerializedName());
            }
            line.add("adjust", adjust);
            line.add("abilities", abilities(stack));
            line.addProperty("sneak_bypass_use", stack.doesSneakBypassUse(e.level(), pos, e.player));
            line.add("patch", patchKeys(stack));
            out.add(line);
        }
        JsonObject radialLine = new JsonObject();
        radialLine.addProperty("case", "radial");
        RadialData<ConfiguratorMode> radial = item.getRadialData(configurator(ConfiguratorMode.CONFIGURATE_ITEMS));
        radialLine.addProperty("id", radial.getIdentifier().toString());
        radialLine.addProperty("modes", radial.getModes().size());
        radialLine.addProperty("default_mode", radial.getDefaultMode(radial.getModes()).getSerializedName());
        JsonArray networkRepresentations = new JsonArray();
        JsonArray indexes = new JsonArray();
        for (ConfiguratorMode mode : radial.getModes()) {
            networkRepresentations.add(radial.getNetworkRepresentation(mode));
            indexes.add(radial.index(radial.getModes(), mode));
        }
        radialLine.add("network_representation", networkRepresentations);
        radialLine.add("index", indexes);
        radialLine.addProperty("same_data_every_call", radial == item.getRadialData(configurator(ConfiguratorMode.WRENCH)));
        out.add(radialLine);
        ItemStack fresh = new ItemStack(MekanismItems.CONFIGURATOR.asItem());
        JsonObject freshLine = new JsonObject();
        freshLine.addProperty("case", "fresh");
        ConfiguratorMode freshMode = fresh.get(MekanismDataComponents.CONFIGURATOR_MODE);
        freshLine.addProperty("mode_component", freshMode == null ? null : freshMode.getSerializedName());
        freshLine.add("item_tooltip", tooltip(e, fresh));
        freshLine.add("patch", patchKeys(fresh));
        out.add(freshLine);
        ItemStack removed = new ItemStack(MekanismItems.CONFIGURATOR.asItem());
        removed.remove(MekanismDataComponents.CONFIGURATOR_MODE.get());
        JsonObject removedLine = new JsonObject();
        removedLine.addProperty("case", "removed");
        ConfiguratorMode removedMode = removed.get(MekanismDataComponents.CONFIGURATOR_MODE);
        removedLine.addProperty("mode_component", removedMode == null ? null : removedMode.getSerializedName());
        removedLine.add("item_tooltip", tooltip(e, removed));
        removedLine.add("patch", patchKeys(removed));
        removedLine.addProperty("get_mode", item.getMode(removed).getSerializedName());
        removedLine.add("abilities", abilities(removed));
        removedLine.addProperty("sneak_bypass_use", removed.doesSneakBypassUse(e.level(), pos, e.player));
        out.add(removedLine);
        out.write();
    }

    private static JsonObject abilities(ItemStack stack) {
        JsonObject abilities = new JsonObject();
        for (ItemAbility ability : ABILITIES) {
            abilities.addProperty(ability.name(), stack.canPerformAction(ability));
        }
        return abilities;
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Arquivo 2: configurator_changemode
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static void changeModeAll(Env e) {
        Out out = new Out("configurator_changemode",
              "ItemConfigurator.changeMode called the way PacketModeChange.handle calls it (ItemAccessUtils.forEntitySlot(player, MAINHAND), a root transaction committed after), from every mode.",
              "mode (the mode before), shift, display (DisplayChange name), canAccess (IItemSecurityUtils.canAccess), after (the mode ItemConfigurator.getMode answers for the main hand stack after the call), changed (after != mode), "
                    + "component (the CONFIGURATOR_MODE component of the main hand stack, serialized name or null), is_default (component equals configurate_items), patch (component patch keys of the stack after), "
                    + "count (stack count), overlay (the ClientboundSystemChatPacket overlay=true messages the player received, Component JSON, in order), chat (the non overlay system messages), "
                    + "payloads (ids of the custom payloads the player received: 'mekanism:show_mode_change' is Mekanism.packetHandler().showModeChange, which DisplayChange.MAIN_HAND calls), show_mode_change (payloads contains it).");
        ItemConfigurator item = MekanismItems.CONFIGURATOR.get();
        for (ConfiguratorMode start : ConfiguratorMode.VALUES) {
            for (int shift : new int[]{-2, -1, 0, 1, 2, 5, 8}) {
                for (DisplayChange display : DisplayChange.values()) {
                    e.hand(configurator(start));
                    e.player.clearOutboundPackets();
                    ItemAccess itemAccess = ItemAccessUtils.forEntitySlot(e.player, EquipmentSlot.MAINHAND);
                    boolean canAccess = false;
                    if (itemAccess.getResource().getItem() instanceof IModeItem modeItem && (canAccess = IItemSecurityUtils.INSTANCE.canAccess(e.player, itemAccess))) {
                        try (Transaction transaction = Transaction.openRoot()) {
                            modeItem.changeMode(e.player, itemAccess, shift, display, transaction);
                            transaction.commit();
                        }
                    }
                    ItemStack stack = e.player.getMainHandItem();
                    JsonObject line = new JsonObject();
                    line.addProperty("mode", start.getSerializedName());
                    line.addProperty("shift", shift);
                    line.addProperty("display", display.name());
                    line.addProperty("canAccess", canAccess);
                    ConfiguratorMode after = item.getMode(stack);
                    line.addProperty("after", after.getSerializedName());
                    line.addProperty("changed", after != start);
                    ConfiguratorMode component = stack.get(MekanismDataComponents.CONFIGURATOR_MODE);
                    line.addProperty("component", component == null ? null : component.getSerializedName());
                    line.addProperty("is_default", component == ConfiguratorMode.CONFIGURATE_ITEMS);
                    line.add("patch", patchKeys(stack));
                    line.addProperty("count", stack.getCount());
                    JsonObject net = drain(e.player);
                    line.add("overlay", net.get("overlay"));
                    line.add("chat", net.get("chat"));
                    line.add("payloads", net.get("payloads"));
                    line.addProperty("show_mode_change", net.getAsJsonArray("payloads").contains(new com.google.gson.JsonPrimitive("mekanism:show_mode_change")));
                    out.add(line);
                }
            }
        }
        out.write();
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Arquivos 3: configurator_click_<alvo>
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final String CLICK_FIELDS = "marks [target, mode, sneak, face]; target; mode (ConfiguratorMode serialized name); sneak; face (the world Direction clicked, the machine faces north: north is its front); "
          + "relative_side (RelativeSide.fromDirections(facing, face) serialized name, only for a tile with a facing); "
          + "result {class (InteractionResult record: Success, Fail, Pass, TryEmptyHandInteraction), consumes_action, swing_source, was_item_interaction, held_item_transformed} of ServerPlayerGameMode.useItemOn(serverPlayer, level, stack, MAIN_HAND, BlockHitResult) "
          + "(the call chain is: onItemUseFirst, then BlockState.useItemOn when the player is not sneaking with an item in hand, BlockState.useWithoutItem when that returns TryEmptyHand, then Item.useOn); "
          + "exception (class and message when the call threw; the case continues); menu (the player's containerMenu after the call: {type: menu type id, class}, or null when none opened; it is closed right after); "
          + "overlay (the overlay messages the player received, Component JSON, in order); chat (the non overlay system messages); payloads (ids of the custom payloads received: an opened Mekanism menu shows 'neoforge:advanced_open_screen'); "
          + "before and after: {block, tile (class simple name or null), facing, control_type, config {type: [DataType x6 in RelativeSide.VALUES order: front,left,right,back,top,bottom]}, ejecting {type: bool}, "
          + "ejector {strict_input, output_color, input_colors[6]}, energy (long as string; the machine starts with 12345), slots [item*count per inventory slot]}; "
          + "block_exists (after.block is the same block as before); drops (ItemEntity stacks in the world within 3 blocks after the call, sorted: {item, count, patch, configurator_mode, configuration_data, side_config, ejector}); "
          + "player_inventory (the non empty slots of the player inventory after the call, same shape plus slot; the dismantled machine goes here via Player.addItem); hand (the main hand stack after the call). "
          + "The machine starts with the item default side configuration, ejecting off, energy 12345, STICK in the input slots (3 in the infusion slot of the infuser, 4 or 5 in the input) and 7 clay balls in the output slot.";

    private static JsonObject clickCase(Env e, String target, Runnable place, ConfiguratorMode mode, boolean sneak, Direction face) {
        place.run();
        e.player.getInventory().clearContent();
        JsonObject line = new JsonObject();
        line.add("marks", marks(target, mode.getSerializedName(), sneak ? "sneak" : "no_sneak", face.getSerializedName()));
        line.addProperty("target", target);
        line.addProperty("mode", mode.getSerializedName());
        line.addProperty("sneak", sneak);
        line.addProperty("face", face.getSerializedName());
        TileEntityMekanism tile = e.tile(P1);
        if (tile != null) {
            line.addProperty("relative_side", RelativeSide.fromDirections(tile.getDirection(), face).getSerializedName());
        }
        line.add("before", snapshot(e, P1));
        e.hand(configurator(mode));
        e.player.setShiftKeyDown(sneak);
        e.player.clearOutboundPackets();
        InteractionResult result = null;
        try {
            result = e.player.gameMode.useItemOn(e.player, e.level(), e.player.getItemInHand(InteractionHand.MAIN_HAND), InteractionHand.MAIN_HAND, e.hit(P1, face));
        } catch (RuntimeException ex) {
            line.addProperty("exception", ex.getClass().getSimpleName() + ": " + ex.getMessage());
        }
        e.player.setShiftKeyDown(false);
        if (result != null) {
            line.add("result", result(result));
        }
        JsonObject net = drain(e.player);
        line.add("menu", openMenu(e.player));
        line.add("overlay", net.get("overlay"));
        line.add("chat", net.get("chat"));
        line.add("payloads", net.get("payloads"));
        e.player.clearOutboundPackets();
        JsonObject before = line.getAsJsonObject("before");
        JsonObject after = snapshot(e, P1);
        line.add("after", after);
        line.addProperty("block_exists", before.get("block").equals(after.get("block")));
        line.add("drops", drops(e, P1));
        line.add("player_inventory", inventory(e));
        line.add("hand", stackJson(e, e.player.getMainHandItem()));
        return line;
    }

    private static void clickMachine(Env e, Spec spec) {
        Out out = new Out("configurator_click_" + spec.name,
              "The Configurator clicked on the " + spec.name + " (placed by the player, facing north) through ServerPlayerGameMode.useItemOn: every mode, sneaking or not, every face.",
              CLICK_FIELDS);
        for (ConfiguratorMode mode : ConfiguratorMode.VALUES) {
            for (boolean sneak : new boolean[]{false, true}) {
                for (Direction face : Direction.values()) {
                    out.add(clickCase(e, spec.name, () -> e.place(spec, P1, Direction.NORTH, true), mode, sneak, face));
                }
            }
        }
        e.clear(P1);
        out.write();
    }

    private static void clickOthers(Env e) {
        Out out = new Out("configurator_click_others",
              "The Configurator clicked on stone, a chest and air through ServerPlayerGameMode.useItemOn: every mode, sneaking or not, every face.",
              CLICK_FIELDS);
        List<Object[]> targets = List.of(new Object[]{"stone", Blocks.STONE}, new Object[]{"chest", Blocks.CHEST}, new Object[]{"air", Blocks.AIR});
        for (Object[] target : targets) {
            Block block = (Block) target[1];
            for (ConfiguratorMode mode : ConfiguratorMode.VALUES) {
                for (boolean sneak : new boolean[]{false, true}) {
                    for (Direction face : Direction.values()) {
                        out.add(clickCase(e, (String) target[0], () -> {
                            e.clear(P1);
                            e.helper.setBlock(P1, block.defaultBlockState());
                        }, mode, sneak, face));
                    }
                }
            }
        }
        e.clear(P1);
        out.write();
    }

    private static void clickSequenceAll(Env e) {
        Out out = new Out("configurator_click_sequence",
              "Twelve sneaking Configurator clicks in a row on the same face, per machine, mode and face (north is the front, up is the top).",
              "marks [machine, mode, face, click N]; machine; mode; face; click (1..12); relative_side; supported (whether the machine supports the transmission type of the mode); "
                    + "data_type (the DataType of that side for the mode's transmission type after the click, null when unsupported or the mode is not a configure mode); "
                    + "result, menu, overlay, chat, payloads as in configurator_click_*; config_after (config of every transmission type after the click); block (still the same machine).");
        for (Spec spec : List.of(chamber(), infuser())) {
            for (ConfiguratorMode mode : ConfiguratorMode.VALUES) {
                if (!mode.isConfigurating()) {
                    continue;
                }
                for (Direction face : List.of(Direction.NORTH, Direction.UP)) {
                    TileEntityMekanism tile = e.place(spec, P1, Direction.NORTH, false);
                    ISideConfiguration side = (ISideConfiguration) tile;
                    RelativeSide relative = RelativeSide.fromDirections(tile.getDirection(), face);
                    e.player.getInventory().clearContent();
                    e.hand(configurator(mode));
                    for (int click = 1; click <= 12; click++) {
                        e.player.setShiftKeyDown(true);
                        e.player.clearOutboundPackets();
                        JsonObject line = new JsonObject();
                        line.add("marks", marks(spec.name, mode.getSerializedName(), face.getSerializedName(), "click " + click));
                        line.addProperty("machine", spec.name);
                        line.addProperty("mode", mode.getSerializedName());
                        line.addProperty("face", face.getSerializedName());
                        line.addProperty("click", click);
                        line.addProperty("relative_side", relative.getSerializedName());
                        boolean supported = side.getConfig().supports(mode.getTransmission());
                        line.addProperty("supported", supported);
                        try {
                            InteractionResult result = e.player.gameMode.useItemOn(e.player, e.level(), e.player.getItemInHand(InteractionHand.MAIN_HAND),
                                  InteractionHand.MAIN_HAND, e.hit(P1, face));
                            line.add("result", result(result));
                        } catch (RuntimeException ex) {
                            line.addProperty("exception", ex.getClass().getSimpleName() + ": " + ex.getMessage());
                        }
                        e.player.setShiftKeyDown(false);
                        JsonObject net = drain(e.player);
                        line.add("menu", openMenu(e.player));
                        line.add("overlay", net.get("overlay"));
                        line.add("chat", net.get("chat"));
                        line.add("payloads", net.get("payloads"));
                        e.player.clearOutboundPackets();
                        line.addProperty("data_type", supported ? dt(side.getConfig().getConfig(mode.getTransmission()).getDataType(relative)) : null);
                        line.add("config_after", configJson(side));
                        line.addProperty("block", BuiltInRegistries.BLOCK.getKey(e.level().getBlockState(e.abs(P1)).getBlock()).toString());
                        out.add(line);
                    }
                }
            }
        }
        e.clear(P1);
        out.write();
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Arquivo 4: configurator_card
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static void setMode(ISideConfiguration machine, TransmissionType type, RelativeSide side, DataType dataType) {
        machine.getConfig().getConfig(type).setDataType(dataType, side);
        machine.getConfig().sideChanged(type, side);
    }

    /// O padrão de `config_lados_card`: o lado `i` de cada tipo recebe o DataType suportado de índice `(i + 1 + ordinal do tipo)`.
    private static void patternA(ISideConfiguration machine) {
        for (TransmissionType type : machine.getConfig().getTransmissions()) {
            List<DataType> supported = new ArrayList<>(machine.getConfig().getConfig(type).getSupportedDataTypes());
            for (int i = 0; i < RelativeSide.VALUES.size(); i++) {
                setMode(machine, type, RelativeSide.VALUES.get(i), supported.get((i + 1 + type.ordinal()) % supported.size()));
            }
        }
    }

    private static void sourceA(TileEntityMekanism tile) {
        if (tile instanceof ISideConfiguration machine) {
            patternA(machine);
            machine.getConfig().getConfig(TransmissionType.ITEM).setEjecting(true);
            machine.getEjector().setStrictInput(true);
            machine.getEjector().setOutputColor(EnumColor.DARK_GREEN);
            machine.getEjector().setInputColor(RelativeSide.LEFT, EnumColor.RED);
            machine.getEjector().setInputColor(RelativeSide.BOTTOM, EnumColor.DARK_BLUE);
        }
        tile.setControlType(RedstoneControl.HIGH);
    }

    private static void targetB(TileEntityMekanism tile) {
        if (tile instanceof ISideConfiguration machine) {
            machine.getEjector().setStrictInput(false);
            machine.getEjector().setOutputColor(EnumColor.YELLOW);
            machine.getEjector().setInputColor(RelativeSide.TOP, EnumColor.PINK);
        }
        tile.setControlType(RedstoneControl.LOW);
    }

    private static final String CARD_FIELDS = "marks [step, ...]; step: read (sneaking click on the source machine A with an empty card), read_overwrite (the same with a card that already holds another machine's data), "
          + "apply (non sneaking click on a default machine B with the card from a read), apply_empty (a card with no data), other_block (stone, chest, air), use_air (ServerPlayerGameMode.useItem), tooltip (the card's tooltip); "
          + "source/target (machine names or block names); face (world Direction clicked; the machines face north); sneak; data (whether the card held data before); "
          + "result {class, consumes_action, swing_source, was_item_interaction, held_item_transformed} of ServerPlayerGameMode.useItemOn (or useItem for use_air); exception; "
          + "menu (the player's containerMenu after the call: {type, class} or null; closed right after); overlay (overlay messages, Component JSON); chat; payloads (custom payload ids); "
          + "card_before and card_after (the stack in hand: item, count, patch, configuration_data = the SNBT of mekanism:configuration_data's compound or null, configuration_json = the same compound as sorted JSON); "
          + "clicked_before/clicked_after (snapshot of the machine or block under the click, A for read and read_overwrite, B for the others: block, tile, facing, control_type, config, ejecting, ejector, energy, slots); "
          + "A is configured: item default sides replaced by the pattern where side i of each type has the supported DataType of index (i + 1 + ordinal of the type) mod count, ejecting on for ITEM, strict input, output color DARK_GREEN, "
          + "input colors LEFT=RED and BOTTOM=DARK_BLUE, redstone HIGH. B is the item default with ejecting off, strict input off, output color YELLOW, input color TOP=PINK, redstone LOW. Both are placed by the player facing north; the Heat Generator only has the redstone mode. "
          + "result_hand (use_air only): the stack in hand after the call. tooltip lines: Component JSON of ItemStack.getTooltipLines (no player, TooltipFlag.NORMAL).";

    private static JsonObject cardStack(Env e, ItemStack stack) {
        JsonObject out = stackJson(e, stack);
        ConfigurationData data = stack.get(MekanismDataComponents.CONFIGURATION_DATA);
        out.add("configuration_json", data == null || data.configuration() == null ? JsonNull.INSTANCE : nbtJson(data.configuration()));
        return out;
    }

    private static ItemStack newCard() {
        return new ItemStack((ItemLike) MekanismItems.CONFIGURATION_CARD);
    }

    private static ItemStack cardWith(ItemStack source) {
        ItemStack copy = newCard();
        ConfigurationData data = source.get(MekanismDataComponents.CONFIGURATION_DATA);
        if (data != null) {
            copy.set(MekanismDataComponents.CONFIGURATION_DATA, data);
        }
        return copy;
    }

    /// Um clique com o cartão na mão em `P1` (a máquina A, a B ou um bloco), pelo caminho do servidor. `b` é a posição da segunda máquina, ou nula.
    private static JsonObject cardClick(Env e, String step, JsonArray marks, ItemStack card, boolean sneak, Direction face, BlockPos clicked, BlockPos other) {
        JsonObject line = new JsonObject();
        line.add("marks", marks);
        line.addProperty("step", step);
        line.addProperty("sneak", sneak);
        line.addProperty("face", face.getSerializedName());
        ConfigurationData held = card.get(MekanismDataComponents.CONFIGURATION_DATA);
        line.addProperty("data", held != null && held.configuration() != null);
        e.player.getInventory().clearContent();
        e.hand(card);
        line.add("card_before", cardStack(e, card));
        line.add("clicked_before", snapshot(e, clicked));
        if (other != null) {
            line.add("other_before", snapshot(e, other));
        }
        e.player.setShiftKeyDown(sneak);
        e.player.clearOutboundPackets();
        try {
            InteractionResult result = e.player.gameMode.useItemOn(e.player, e.level(), e.player.getItemInHand(InteractionHand.MAIN_HAND), InteractionHand.MAIN_HAND,
                  e.hit(clicked, face));
            line.add("result", result(result));
        } catch (RuntimeException ex) {
            line.addProperty("exception", ex.getClass().getSimpleName() + ": " + ex.getMessage());
        }
        e.player.setShiftKeyDown(false);
        JsonObject net = drain(e.player);
        line.add("menu", openMenu(e.player));
        line.add("overlay", net.get("overlay"));
        line.add("chat", net.get("chat"));
        line.add("payloads", net.get("payloads"));
        e.player.clearOutboundPackets();
        line.add("card_after", cardStack(e, e.player.getMainHandItem()));
        line.add("clicked_after", snapshot(e, clicked));
        if (other != null) {
            line.add("other_after", snapshot(e, other));
        }
        return line;
    }

    private static String sneakName(boolean sneak) {
        return sneak ? "sneak" : "no_sneak";
    }

    private static void cardAll(Env e) {
        Out out = new Out("configurator_card",
              "The configuration card through ServerPlayerGameMode.useItemOn and useItem: reading a configured machine, applying to a default one, other machine types, an empty card, other blocks, use in the air, tooltips.",
              CARD_FIELDS);
        // 1. leitura: clique agachado na A, com o cartão vazio, em cada face
        for (Spec spec : allMachines()) {
            for (Direction face : Direction.values()) {
                sourceMachine(e, spec);
                JsonObject line = cardClick(e, "read", marks("read", spec.name, face.getSerializedName(), "sneak"), newCard(), true, face, P1, null);
                line.addProperty("source", spec.name);
                out.add(line);
            }
        }
        // 2. leitura sobre um cartão que já tem dados (os da Metallurgic Infuser) na Enrichment Chamber
        {
            ItemStack infuserCard = readCard(e, infuser());
            sourceMachine(e, chamber());
            JsonObject line = cardClick(e, "read_overwrite", marks("read_overwrite", "enrichment_chamber", "up", "sneak"), infuserCard, true, Direction.UP, P1, null);
            line.addProperty("source", "enrichment_chamber");
            out.add(line);
        }
        // 3. aplicação: clique não agachado numa máquina padrão B, com o cartão lido de uma A
        for (Spec source : allMachines()) {
            ItemStack card = readCard(e, source);
            for (Spec target : allMachines()) {
                List<Direction> faces = source == target || source.name.equals(target.name) ? List.of(Direction.values()) : List.of(Direction.UP);
                for (Direction face : faces) {
                    TileEntityMekanism b = e.place(target, P1, Direction.NORTH, true);
                    targetB(b);
                    JsonObject line = cardClick(e, "apply", marks("apply", source.name, "to", target.name, face.getSerializedName(), "no_sneak"), cardWith(card), false, face, P1, null);
                    line.addProperty("source", source.name);
                    line.addProperty("target", target.name);
                    out.add(line);
                }
            }
        }
        // 3b. aplicação agachada (o agachado lê, não aplica): o cartão com dados numa máquina padrão, agachado
        {
            ItemStack card = readCard(e, chamber());
            TileEntityMekanism b = e.place(chamber(), P1, Direction.NORTH, true);
            targetB(b);
            JsonObject line = cardClick(e, "apply", marks("apply", "enrichment_chamber", "to", "enrichment_chamber", "up", "sneak"), cardWith(card), true, Direction.UP, P1, null);
            line.addProperty("source", "enrichment_chamber");
            line.addProperty("target", "enrichment_chamber");
            out.add(line);
        }
        // 4. cartão vazio, não agachado, em cada máquina
        for (Spec target : allMachines()) {
            TileEntityMekanism b = e.place(target, P1, Direction.NORTH, true);
            targetB(b);
            JsonObject line = cardClick(e, "apply_empty", marks("apply_empty", target.name, "up", "no_sneak"), newCard(), false, Direction.UP, P1, null);
            line.addProperty("target", target.name);
            out.add(line);
        }
        // 5. outros blocos, com e sem dados, agachado e não
        ItemStack chamberCard = readCard(e, chamber());
        List<Object[]> blocks = List.of(new Object[]{"stone", Blocks.STONE}, new Object[]{"chest", Blocks.CHEST}, new Object[]{"air", Blocks.AIR});
        for (Object[] target : blocks) {
            Block block = (Block) target[1];
            for (boolean withData : new boolean[]{false, true}) {
                for (boolean sneak : new boolean[]{false, true}) {
                    e.clear(P1);
                    e.helper.setBlock(P1, block.defaultBlockState());
                    JsonObject line = cardClick(e, "other_block", marks("other_block", (String) target[0], withData ? "with_data" : "empty_card", "up", sneakName(sneak)),
                          withData ? cardWith(chamberCard) : newCard(), sneak, Direction.UP, P1, null);
                    line.addProperty("target", (String) target[0]);
                    out.add(line);
                }
            }
        }
        e.clear(P1);
        // 6. uso no ar (ServerPlayerGameMode.useItem)
        for (boolean withData : new boolean[]{false, true}) {
            for (boolean sneak : new boolean[]{false, true}) {
                ItemStack card = withData ? cardWith(chamberCard) : newCard();
                e.player.getInventory().clearContent();
                e.hand(card);
                JsonObject line = new JsonObject();
                line.add("marks", marks("use_air", withData ? "with_data" : "empty_card", sneakName(sneak)));
                line.addProperty("step", "use_air");
                line.addProperty("sneak", sneak);
                line.addProperty("data", withData);
                line.add("card_before", cardStack(e, card));
                e.player.setShiftKeyDown(sneak);
                e.player.clearOutboundPackets();
                try {
                    InteractionResult result = e.player.gameMode.useItem(e.player, e.level(), e.player.getItemInHand(InteractionHand.MAIN_HAND), InteractionHand.MAIN_HAND);
                    line.add("result", result(result));
                } catch (RuntimeException ex) {
                    line.addProperty("exception", ex.getClass().getSimpleName() + ": " + ex.getMessage());
                }
                e.player.setShiftKeyDown(false);
                JsonObject net = drain(e.player);
                line.add("menu", openMenu(e.player));
                line.add("overlay", net.get("overlay"));
                line.add("chat", net.get("chat"));
                line.add("payloads", net.get("payloads"));
                e.player.clearOutboundPackets();
                line.add("card_after", cardStack(e, e.player.getMainHandItem()));
                out.add(line);
            }
        }
        // 7. tooltips
        for (String which : List.of("empty", "enrichment_chamber", "heat_generator")) {
            ItemStack card = which.equals("empty") ? newCard() : cardWith(readCard(e, which.equals("heat_generator") ? heat() : chamber()));
            JsonObject line = new JsonObject();
            line.add("marks", marks("tooltip", which));
            line.addProperty("step", "tooltip");
            line.addProperty("data", !which.equals("empty"));
            line.add("card", cardStack(e, card));
            line.add("tooltip", tooltip(e, card));
            out.add(line);
        }
        e.clear(P1);
        out.write();
    }

    private static void sourceMachine(Env e, Spec spec) {
        TileEntityMekanism a = e.place(spec, P1, Direction.NORTH, true);
        sourceA(a);
    }

    /// Lê uma máquina A com um cartão novo, agachado, pelo caminho do servidor, e devolve o cartão.
    private static ItemStack readCard(Env e, Spec spec) {
        sourceMachine(e, spec);
        ItemStack card = newCard();
        e.player.getInventory().clearContent();
        e.hand(card);
        e.player.setShiftKeyDown(true);
        e.player.gameMode.useItemOn(e.player, e.level(), e.player.getItemInHand(InteractionHand.MAIN_HAND), InteractionHand.MAIN_HAND, e.hit(P1, Direction.UP));
        e.player.setShiftKeyDown(false);
        ItemStack read = e.player.getMainHandItem();
        e.player.clearOutboundPackets();
        if (e.player.containerMenu != e.player.inventoryMenu) {
            e.player.closeContainer();
        }
        e.player.clearOutboundPackets();
        return cardWith(read);
    }
}
