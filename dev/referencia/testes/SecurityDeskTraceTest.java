package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.Codec;
import com.mojang.serialization.JsonOps;
import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.TreeMap;
import java.util.UUID;
import mekanism.api.AutomationType;
import mekanism.api.inventory.IInventorySlot;
import mekanism.api.security.IBlockSecurityUtils;
import mekanism.api.security.ISecurityObject;
import mekanism.api.security.SecurityMode;
import mekanism.client.MekanismClient;
import mekanism.common.inventory.container.sync.ISyncableData;
import mekanism.common.inventory.container.sync.SyncableStreamCodec;
import mekanism.common.inventory.slot.SecurityInventorySlot;
import mekanism.common.lib.frequency.FrequencyLookup;
import mekanism.common.lib.frequency.FrequencyTypes;
import mekanism.common.lib.security.SecurityData;
import mekanism.common.lib.security.SecurityFrequency;
import mekanism.common.lib.security.SecurityUtils;
import mekanism.common.network.to_client.security.PacketBatchSecurityUpdate;
import mekanism.common.network.to_client.security.PacketSyncSecurity;
import mekanism.common.network.to_server.PacketGuiInteract.GuiInteraction;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismDataComponents;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.TileEntitySecurityDesk;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.util.text.InputValidator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.RegistryAccess;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.RegistryOps;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.players.NameAndId;
import net.minecraft.server.players.UserNameToIdResolver;
import net.minecraft.util.StringUtil;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava a Security Desk (`TileEntitySecurityDesk`, `SecurityFrequency`, `FrequencyLookup`, `SecurityUtils`, `SecurityInventorySlot`): `security_desk.jsonl`, uma linha
/// por passo. O dono põe a mesa e uma máquina; cada passo mexe na frequência (adicionar e tirar um nome, trocar o modo, ligar a sobreposição) e grava a frequência (o
/// NBT do `CODEC`, o do `FrequencyLookup` inteiro e o id do arquivo, os bytes do `STREAM_CODEC`, o `hashCode` e o `getSyncHash`), o `PacketSyncSecurity` e o
/// `PacketBatchSecurityUpdate`, se o dado do menu ficou sujo (`SyncableStreamCodec.frequency(...).isDirty()`), e a matriz de acesso: o dono, o convidado, o estranho e o
/// UUID nulo contra a máquina nos três modos, no servidor, no cliente que recebeu o `PacketSyncSecurity` e no cliente que não recebeu. O passo `slots` grava o que os
/// dois slots da mesa fazem (`lock`, `unlock`, e o que cada automação insere e tira), e `names` o que o `PacketAddTrusted` aceita como nome.
@ForEachTest(groups = "trace")
public class SecurityDeskTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final BlockPos DESK_FLOOR = new BlockPos(1, 0, 1);
    private static final BlockPos DESK = new BlockPos(1, 1, 1);
    private static final BlockPos MACHINE_FLOOR = new BlockPos(3, 0, 1);
    private static final BlockPos MACHINE = new BlockPos(3, 1, 1);
    private static final String GUEST_NAME = "Convidado";
    private static final String UNKNOWN_NAME = "Zz_Nunca_Visto1";

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_security_desk")
    @TestHolder(description = "Records the Security Desk: the frequency, the trusted list, the override, the access matrix, the packets and the slots.")
    public static void desk(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(5, 4, 3)
              .fill(1, 0, 1, 1, 0, 1, Blocks.STONE.defaultBlockState())
              .fill(3, 0, 1, 3, 0, 1, Blocks.STONE.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> new Scene(helper).run());
            sequence.thenSucceed();
        });
    }

    private static final class Scene {
        private final MekGameTestHelper helper;
        private final RegistryAccess registries;
        private final RegistryOps<JsonElement> json;
        private final RegistryOps<Tag> nbt;
        private final List<String> lines = new ArrayList<>();
        private final Player owner;
        private final Player guest;
        private final Player stranger;
        private SyncableStreamCodec<SecurityFrequency> menu;

        Scene(MekGameTestHelper helper) {
            this.helper = helper;
            this.registries = helper.getLevel().registryAccess();
            this.json = RegistryOps.create(JsonOps.INSTANCE, registries);
            this.nbt = RegistryOps.create(NbtOps.INSTANCE, registries);
            this.owner = helper.makeMockPlayerLookingAt(DESK_FLOOR, Direction.UP);
            this.guest = helper.makeTickingMockServerPlayerInLevel(GameType.SURVIVAL);
            this.stranger = helper.makeTickingMockServerPlayerInLevel(GameType.SURVIVAL);
        }

        private TileEntitySecurityDesk desk() {
            return (TileEntitySecurityDesk) helper.getLevel().getBlockEntity(helper.absolutePos(DESK));
        }

        private TileEntityMekanism machine() {
            return (TileEntityMekanism) helper.getLevel().getBlockEntity(helper.absolutePos(MACHINE));
        }

        private JsonElement toJson(Tag tag) {
            return NbtOps.INSTANCE.convertTo(JsonOps.INSTANCE, tag);
        }

        private static String hex(ByteBuf buffer) {
            StringBuilder out = new StringBuilder();
            while (buffer.isReadable()) {
                out.append(String.format("%02x", buffer.readByte()));
            }
            return out.toString();
        }

        void run() {
            ServerLevel level = helper.getLevel();
            UserNameToIdResolver cache = level.getServer().services().nameToIdCache();
            // Os três jogadores de teste têm o mesmo nome (`test-mock-player`): o cache de nomes ganha um nome por jogador, como o login de um jogador de verdade faz.
            cache.add(new NameAndId(owner.getUUID(), "Dono"));
            cache.add(new NameAndId(guest.getUUID(), GUEST_NAME));
            cache.add(new NameAndId(stranger.getUUID(), "Estranho"));

            helper.useOn(DESK_FLOOR, new ItemStack(MekanismBlocks.SECURITY_DESK.asItem()), owner, Direction.UP);
            helper.useOn(MACHINE_FLOOR, new ItemStack(MekanismBlocks.CRUSHER.asItem()), owner, Direction.UP);
            SecurityFrequency frequency = desk().getFreq();
            menu = SyncableStreamCodec.frequency(FrequencyTypes.SECURITY, () -> desk().getFreq(), value -> {
            });

            JsonObject placed = step("placed");
            BlockPos abs = helper.absolutePos(DESK);
            placed.addProperty("state", level.getBlockState(abs).toString());
            placed.addProperty("above", level.getBlockState(abs.above()).toString());
            BlockEntity above = level.getBlockEntity(abs.above());
            placed.add("above_nbt", above == null ? null : toJson(above.saveWithFullMetadata(registries)));
            placed.add("desk_nbt", toJson(desk().saveWithFullMetadata(registries)));
            placed.add("desk_update_tag", toJson(desk().getUpdateTag(registries)));
            placed.addProperty("lookup_id", FrequencyLookup.getId(null, SecurityMode.PUBLIC, FrequencyTypes.SECURITY).toString());
            placed.addProperty("frequency_is_lookup", frequency == FrequencyTypes.SECURITY.getFrequency(null, SecurityMode.PUBLIC, owner.getUUID()));
            JsonObject deskAccess = new JsonObject();
            BlockState deskState = level.getBlockState(abs);
            for (Object[] who : players()) {
                Player player = (Player) who[1];
                deskAccess.addProperty((String) who[0], IBlockSecurityUtils.INSTANCE.canAccess(player, level, abs, deskState, desk()));
                deskAccess.addProperty((String) who[0] + "_open", String.valueOf(desk().openGui(level, player)));
                deskAccess.addProperty((String) who[0] + "_break", deskState.getDestroyProgress(player, level, abs));
            }
            placed.add("desk_access", deskAccess);
            finish(placed);

            JsonObject names = new JsonObject();
            names.addProperty("step", "names");
            for (String name : new String[]{"", "Convidado", "convidado", "a b", "nome-hifen", "aço", "abcdefghijklmnop", "abcdefghijklmnopq", "_", "x", " Convidado "}) {
                JsonObject entry = new JsonObject();
                entry.addProperty("validator", InputValidator.test(name, InputValidator.USERNAME));
                entry.addProperty("valid_player_name", StringUtil.isValidPlayerName(name));
                entry.addProperty("trim", name.trim());
                names.add(name, entry);
            }
            Optional<NameAndId> lower = cache.get("convidado");
            names.addProperty("cache_lower", lower.map(profile -> profile.id() + " " + profile.name()).orElse("empty"));
            names.addProperty("guest_uuid", guest.getUUID().toString());
            lines.add(names.toString());

            desk().addTrusted(GUEST_NAME);
            finish(step("add_guest"));
            desk().addTrusted("convidado");
            finish(step("add_guest_lowercase"));
            desk().addTrusted(UNKNOWN_NAME);
            JsonObject unknown = step("add_unknown");
            Optional<NameAndId> resolved = cache.get(UNKNOWN_NAME);
            unknown.addProperty("resolved", resolved.map(profile -> profile.id() + " " + profile.name()).orElse("empty"));
            unknown.addProperty("offline_lower", NameAndId.createOffline(UNKNOWN_NAME.toLowerCase(java.util.Locale.ROOT)).id().toString());
            unknown.addProperty("offline_exact", NameAndId.createOffline(UNKNOWN_NAME).id().toString());
            finish(unknown);
            desk().addTrusted("Dono");
            finish(step("add_owner"));

            desk().setSecurityDeskMode(SecurityMode.TRUSTED);
            finish(step("mode_trusted"));
            desk().setSecurityDeskMode(SecurityMode.TRUSTED);
            finish(step("mode_trusted_again"));
            desk().setSecurityDeskMode(SecurityMode.PRIVATE);
            finish(step("mode_private"));
            desk().toggleOverride();
            finish(step("override_on_private"));
            desk().setSecurityDeskMode(SecurityMode.TRUSTED);
            finish(step("override_on_trusted"));
            desk().setSecurityDeskMode(SecurityMode.PUBLIC);
            finish(step("override_on_public"));
            desk().setSecurityDeskMode(SecurityMode.TRUSTED);
            desk().removeTrusted(7);
            finish(step("remove_out_of_range"));
            desk().removeTrusted(-1);
            finish(step("remove_negative"));
            desk().removeTrusted(1);
            finish(step("remove_unknown"));
            desk().removeTrusted(0);
            finish(step("remove_guest"));
            desk().addTrusted(GUEST_NAME);
            finish(step("add_guest_back"));
            desk().toggleOverride();
            finish(step("override_off"));

            // O que o servidor aceita de quem não é o dono: os três `GuiInteraction` da mesa não conferem o dono (o cliente é que desliga os botões).
            GuiInteraction.SECURITY_DESK_MODE.consume(level, desk(), stranger, SecurityMode.PRIVATE.ordinal());
            finish(step("stranger_packet_mode_private"));
            GuiInteraction.OVERRIDE_BUTTON.consume(level, desk(), stranger, 0);
            finish(step("stranger_packet_override"));
            GuiInteraction.REMOVE_TRUSTED.consume(level, desk(), stranger, 0);
            finish(step("stranger_packet_remove"));
            GuiInteraction.OVERRIDE_BUTTON.consume(level, desk(), owner, 0);
            GuiInteraction.SECURITY_DESK_MODE.consume(level, desk(), owner, SecurityMode.TRUSTED.ordinal());
            finish(step("owner_packets_back"));

            lines.add(slots().toString());

            JsonObject saved = step("saved");
            saved.add("desk_nbt", toJson(desk().saveWithFullMetadata(registries)));
            saved.add("machine_nbt", toJson(machine().saveWithFullMetadata(registries)));
            finish(saved);
            write("security_desk", lines);
        }

        private List<Object[]> players() {
            List<Object[]> out = new ArrayList<>();
            out.add(new Object[]{"owner", owner});
            out.add(new Object[]{"guest", guest});
            out.add(new Object[]{"stranger", stranger});
            return out;
        }

        private JsonObject step(String name) {
            JsonObject root = new JsonObject();
            root.addProperty("step", name);
            root.addProperty("owner", owner.getUUID().toString());
            root.addProperty("guest", guest.getUUID().toString());
            root.addProperty("stranger", stranger.getUUID().toString());
            return root;
        }

        private void finish(JsonObject root) {
            SecurityFrequency frequency = desk().getFreq();
            root.add("frequency", frequency(frequency));
            FrequencyLookup<SecurityFrequency> lookup = FrequencyTypes.SECURITY.getLookup(null, SecurityMode.PUBLIC);
            Codec<FrequencyLookup<SecurityFrequency>> codec = FrequencyLookup.codec(FrequencyTypes.SECURITY, FrequencyLookup.baseCodec(FrequencyTypes.SECURITY), SecurityMode.PUBLIC);
            root.add("lookup", toJson(codec.encodeStart(nbt, lookup).getOrThrow()));
            root.addProperty("lookup_dirty", lookup.isDirty());
            lookup.setDirty(false);
            root.addProperty("menu_dirty", menu.isDirty().name());
            root.addProperty("menu_bytes", propertyBytes());
            ByteBuf packet = Unpooled.buffer();
            PacketSyncSecurity sync = new PacketSyncSecurity(frequency);
            PacketSyncSecurity.STREAM_CODEC.encode(packet, sync);
            root.addProperty("sync_packet", hex(packet));
            ByteBuf placedPacket = Unpooled.buffer();
            PacketSyncSecurity.STREAM_CODEC.encode(placedPacket, new PacketSyncSecurity(owner.getUUID()));
            root.addProperty("sync_packet_placed", hex(placedPacket));
            PacketBatchSecurityUpdate batch = new PacketBatchSecurityUpdate();
            JsonObject batchJson = new JsonObject();
            for (Map.Entry<UUID, SecurityData> entry : new TreeMap<>(batch.securityMap()).entrySet()) {
                JsonObject data = new JsonObject();
                data.addProperty("mode", entry.getValue().mode().name());
                data.addProperty("override", entry.getValue().override());
                data.addProperty("name", batch.uuidMap().get(entry.getKey()));
                batchJson.add(entry.getKey().toString(), data);
            }
            root.add("batch", batchJson);
            root.add("access", access(frequency));
            lines.add(root.toString());
        }

        private String propertyBytes() {
            try {
                Object property = menu.getPropertyData(registries, (short) 0, ISyncableData.DirtyType.DIRTY);
                Field field = property.getClass().getDeclaredField("value");
                field.setAccessible(true);
                StringBuilder hex = new StringBuilder();
                for (byte b : (byte[]) field.get(property)) {
                    hex.append(String.format("%02x", b));
                }
                return hex.toString();
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private JsonObject frequency(SecurityFrequency frequency) {
            JsonObject out = new JsonObject();
            out.add("nbt", toJson(SecurityFrequency.CODEC.encodeStart(nbt, frequency).getOrThrow()));
            ByteBuf stream = Unpooled.buffer();
            SecurityFrequency.STREAM_CODEC.encode(stream, frequency);
            ByteBuf copy = stream.copy();
            out.addProperty("stream", hex(stream));
            SecurityFrequency decoded = SecurityFrequency.STREAM_CODEC.decode(copy);
            JsonArray decodedNames = new JsonArray();
            decoded.getTrustedUsernameCache().forEach(decodedNames::add);
            out.add("client_names", decodedNames);
            out.addProperty("client_mode", decoded.getSecurity().name());
            out.addProperty("client_override", decoded.isOverridden());
            out.addProperty("client_owner_name", decoded.getOwnerName());
            out.addProperty("client_hash", decoded.hashCode());
            out.addProperty("client_sync_hash", decoded.getSyncHash());
            out.addProperty("hash", frequency.hashCode());
            out.addProperty("sync_hash", frequency.getSyncHash());
            out.addProperty("mode", frequency.getSecurity().name());
            out.addProperty("override", frequency.isOverridden());
            out.addProperty("owner_name", frequency.getOwnerName());
            out.addProperty("valid", frequency.isValid());
            JsonArray names = new JsonArray();
            frequency.getTrustedUsernameCache().forEach(names::add);
            out.add("names", names);
            JsonObject trusted = new JsonObject();
            trusted.addProperty("owner", frequency.isTrusted(owner.getUUID()));
            trusted.addProperty("guest", frequency.isTrusted(guest.getUUID()));
            trusted.addProperty("stranger", frequency.isTrusted(stranger.getUUID()));
            out.add("trusted", trusted);
            return out;
        }

        /// A matriz de acesso à máquina do dono nos três modos dela: o servidor, o cliente com o `PacketSyncSecurity` desta frequência (`clientSecurityMap`) e o
        /// cliente sem ele.
        private JsonObject access(SecurityFrequency frequency) {
            ServerLevel level = helper.getLevel();
            BlockPos abs = helper.absolutePos(MACHINE);
            TileEntityMekanism tile = machine();
            ISecurityObject security = IBlockSecurityUtils.INSTANCE.securityCapability(level, abs, tile);
            JsonObject out = new JsonObject();
            for (SecurityMode mode : SecurityMode.values()) {
                tile.getSecurity().setMode(mode);
                JsonObject entry = new JsonObject();
                side(entry, "server", security, false);
                try {
                    MekanismClient.clientSecurityMap.put(owner.getUUID(), new SecurityData(frequency));
                    side(entry, "client_synced", security, true);
                    MekanismClient.clientSecurityMap.clear();
                    side(entry, "client_unsynced", security, true);
                } catch (Throwable error) {
                    entry.addProperty("client_error", String.valueOf(error));
                }
                out.add(mode.name(), entry);
            }
            tile.getSecurity().setMode(SecurityMode.PUBLIC);
            return out;
        }

        private void side(JsonObject entry, String side, ISecurityObject security, boolean client) {
            SecurityUtils utils = SecurityUtils.get();
            JsonObject out = new JsonObject();
            out.addProperty("owner", utils.canAccessObject(owner.getUUID(), security, client));
            out.addProperty("guest", utils.canAccessObject(guest.getUUID(), security, client));
            out.addProperty("stranger", utils.canAccessObject(stranger.getUUID(), security, client));
            out.addProperty("null", utils.canAccessObject((UUID) null, security, client));
            SecurityData data = utils.getFinalData(security, client);
            out.addProperty("mode", data.mode().name());
            out.addProperty("override", data.override());
            entry.add(side, out);
        }

        private ItemStack machineItem(UUID stackOwner, SecurityMode mode) {
            ItemStack stack = new ItemStack(MekanismBlocks.CRUSHER.asItem());
            if (stackOwner != null) {
                stack.set(MekanismDataComponents.OWNER, stackOwner);
            }
            if (mode != null) {
                stack.set(MekanismDataComponents.SECURITY, mode);
            }
            return stack;
        }

        private JsonElement stack(IInventorySlot slot) {
            ItemStack stack = slot.asItemStack();
            if (stack.isEmpty()) {
                return new JsonObject();
            }
            return ItemStack.CODEC.encodeStart(json, stack).getOrThrow();
        }

        /// Os dois slots da mesa (`SecurityInventorySlot`): o de cima destrava (tira o dono e põe `PUBLIC` no item do dono da mesa) e o de baixo trava (dá o dono da
        /// mesa a um item sem dono e o modo da frequência ao item do dono). E o que cada automação insere e tira de cada um.
        private JsonObject slots() {
            JsonObject out = new JsonObject();
            out.addProperty("step", "slots");
            TileEntitySecurityDesk desk = desk();
            SecurityFrequency frequency = desk.getFreq();
            out.addProperty("frequency_mode", frequency.getSecurity().name());
            List<IInventorySlot> slots = desk.getInventorySlots();
            out.addProperty("count", slots.size());
            SecurityInventorySlot unlock = (SecurityInventorySlot) slots.get(0);
            SecurityInventorySlot lock = (SecurityInventorySlot) slots.get(1);
            Object[][] items = {
                  {"unowned", machineItem(null, null)},
                  {"owner_private", machineItem(owner.getUUID(), SecurityMode.PRIVATE)},
                  {"owner_public", machineItem(owner.getUUID(), null)},
                  {"stranger_private", machineItem(stranger.getUUID(), SecurityMode.PRIVATE)},
                  {"stone", new ItemStack(Items.STONE)},
            };
            for (Object[] item : items) {
                ItemStack stack = (ItemStack) item[1];
                JsonObject entry = new JsonObject();
                entry.add("stack", ItemStack.CODEC.encodeStart(json, stack).getOrThrow());
                for (Object[] which : new Object[][]{{"unlock", unlock}, {"lock", lock}}) {
                    SecurityInventorySlot slot = (SecurityInventorySlot) which[1];
                    String name = (String) which[0];
                    for (AutomationType type : AutomationType.values()) {
                        slot.setContents(ItemResource.EMPTY, 0, null);
                        int inserted;
                        try (Transaction transaction = Transaction.openRoot()) {
                            inserted = slot.insert(ItemResource.of(stack), 1, transaction, type);
                        }
                        entry.addProperty(name + "_insert_" + type.name(), inserted);
                        if (stack.is(Items.STONE)) {
                            continue;
                        }
                        slot.setContents(ItemResource.of(stack), 1, null);
                        int extracted;
                        try (Transaction transaction = Transaction.openRoot()) {
                            extracted = slot.extract(ItemResource.of(stack), 1, transaction, type);
                        }
                        entry.addProperty(name + "_extract_" + type.name(), extracted);
                    }
                    if (stack.is(Items.STONE)) {
                        continue;
                    }
                    slot.setContents(ItemResource.of(stack), 1, null);
                    if (name.equals("unlock")) {
                        slot.unlock(owner.getUUID(), null);
                    } else {
                        slot.lock(owner.getUUID(), frequency, null);
                    }
                    entry.add(name + "_after", stack(slot));
                    slot.setContents(ItemResource.EMPTY, 0, null);
                }
                out.add((String) item[0], entry);
            }
            return out;
        }
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
}
