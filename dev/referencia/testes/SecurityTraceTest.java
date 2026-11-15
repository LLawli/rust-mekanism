package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.lang.reflect.Field;
import java.util.UUID;
import java.util.function.Supplier;
import mekanism.api.security.IBlockSecurityUtils;
import mekanism.api.security.ISecurityObject;
import mekanism.api.security.ISecurityUtils;
import mekanism.api.security.SecurityMode;
import mekanism.common.MekanismLang;
import mekanism.common.inventory.container.sync.ISyncableData;
import mekanism.common.inventory.container.sync.SyncableStreamCodec;
import mekanism.common.lib.frequency.FrequencyTypes;
import mekanism.common.lib.security.SecurityFrequency;
import mekanism.common.lib.security.SecurityUtils;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.util.MekanismUtils;
import mekanism.api.text.EnumColor;
import mekanism.generators.common.registries.GeneratorsBlocks;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.RegistryAccess;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.resources.RegistryOps;
import net.minecraft.network.protocol.game.ServerboundPlayerActionPacket;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.GameType;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o componente de segurança (`TileComponentSecurity`, `SecurityUtils`, `BlockMekanism.getDestroyProgress`, `TileEntityMekanism.openGui`) de uma máquina posta por um
/// jogador: `security_<máquina>.jsonl`, uma linha por modo (`public`, `private`, `trusted`) com o NBT do tile, quem acessa (o dono e outro jogador, no servidor e no
/// cliente, e o UUID nulo), o progresso de quebra de cada um, o resultado de `openGui`, a mensagem de acesso negado, o item solto (componentes e tooltip visto pelo dono e
/// pelo outro jogador), o dono do tile posto de volta por cada um dos dois, e o que o clique da aba (`incrementSecurityMode`, `decrementSecurityMode`) faz para cada um.
@ForEachTest(groups = "trace")
public class SecurityTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final BlockPos SPOT = new BlockPos(3, 1, 1);
    private static final BlockPos FLOOR = new BlockPos(3, 0, 1);

    private record Spec(String name, Supplier<ItemLike> item) {
    }

    private static final Spec CRUSHER = new Spec("crusher", () -> MekanismBlocks.CRUSHER.asItem());
    private static final Spec HEAT = new Spec("heat_generator", () -> GeneratorsBlocks.HEAT_GENERATOR.asItem());
    private static final Spec CUBE = new Spec("basic_energy_cube", () -> MekanismBlocks.BASIC_ENERGY_CUBE.asItem());

    private static void template(DynamicTest test, Spec spec) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(6, 3, 3).fill(3, 0, 1, 3, 0, 1, Blocks.STONE.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> {
            var sequence = helper.startSequence();
            sequence.thenExecute(() -> new Scene(helper, spec).run());
            sequence.thenSucceed();
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_security")
    @TestHolder(description = "Records the security component of the Crusher: owner, modes, access, break, open and the item.")
    public static void crusher(final DynamicTest test) {
        template(test, CRUSHER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_security")
    @TestHolder(description = "Records the security component of the Heat Generator: owner, modes, access, break, open and the item.")
    public static void heat(final DynamicTest test) {
        template(test, HEAT);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_security")
    @TestHolder(description = "Records the security component of the Basic Energy Cube: owner, modes, access, break, open and the item.")
    public static void cube(final DynamicTest test) {
        template(test, CUBE);
    }

    private static final class Scene {
        private final MekGameTestHelper helper;
        private final Spec spec;
        private final RegistryAccess registries;
        private final RegistryOps<JsonElement> json;
        private final List<String> lines = new ArrayList<>();
        private final Player owner;
        private final Player other;

        Scene(MekGameTestHelper helper, Spec spec) {
            this.helper = helper;
            this.spec = spec;
            this.registries = helper.getLevel().registryAccess();
            this.json = RegistryOps.create(JsonOps.INSTANCE, registries);
            this.owner = helper.makeMockPlayerLookingAt(FLOOR, Direction.UP);
            this.other = helper.makeTickingMockServerPlayerInLevel(GameType.SURVIVAL);
        }

        void run() {
            for (SecurityMode mode : SecurityMode.values()) {
                helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
                helper.useOn(FLOOR, new ItemStack(spec.item.get()), owner, Direction.UP);
                TileEntityMekanism tile = tile();
                tile.getSecurity().setMode(mode);
                lines.add(line(mode, tile).toString());
                helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
            }
            write("security_" + spec.name, lines);
        }

        private TileEntityMekanism tile() {
            BlockEntity entity = helper.getLevel().getBlockEntity(helper.absolutePos(SPOT));
            return (TileEntityMekanism) entity;
        }

        private JsonElement component(Component text) {
            return ComponentSerialization.CODEC.encodeStart(json, text).getOrThrow();
        }

        private JsonObject line(SecurityMode mode, TileEntityMekanism tile) {
            ServerLevel level = helper.getLevel();
            BlockPos abs = helper.absolutePos(SPOT);
            BlockState state = level.getBlockState(abs);
            ISecurityObject security = IBlockSecurityUtils.INSTANCE.securityCapability(level, abs, tile);
            JsonObject root = new JsonObject();
            root.addProperty("mode", mode.name());
            root.addProperty("owner", owner.getUUID().toString());
            root.addProperty("other", other.getUUID().toString());
            root.addProperty("owner_name", owner.getName().getString());
            CompoundTag tag = tile.saveWithFullMetadata(registries);
            root.add("nbt", NbtOps.INSTANCE.convertTo(JsonOps.INSTANCE, tag));
            JsonObject access = new JsonObject();
            for (boolean client : new boolean[]{false, true}) {
                String side = client ? "client" : "server";
                access.addProperty("owner_" + side, SecurityUtils.get().canAccessObject(owner.getUUID(), security, client));
                access.addProperty("other_" + side, SecurityUtils.get().canAccessObject(other.getUUID(), security, client));
                access.addProperty("null_" + side, SecurityUtils.get().canAccessObject((UUID) null, security, client));
                access.addProperty("mode_" + side, SecurityUtils.get().getEffectiveSecurityMode(security, client).name());
            }
            access.addProperty("owner_block", IBlockSecurityUtils.INSTANCE.canAccess(owner, level, abs, state, tile));
            access.addProperty("other_block", IBlockSecurityUtils.INSTANCE.canAccess(other, level, abs, state, tile));
            root.add("access", access);
            JsonObject use = new JsonObject();
            use.addProperty("owner_break", state.getDestroyProgress(owner, level, abs));
            use.addProperty("other_break", state.getDestroyProgress(other, level, abs));
            use.addProperty("owner_open", String.valueOf(tile.openGui(level, owner)));
            use.addProperty("other_open", String.valueOf(tile.openGui(level, other)));
            use.add("no_access", component(MekanismUtils.logFormat(EnumColor.RED, MekanismLang.NO_ACCESS)));
            root.add("use", use);
            List<ItemStack> drops = Block.getDrops(state, level, abs, tile);
            JsonArray array = new JsonArray();
            ItemStack dropped = null;
            for (ItemStack drop : drops) {
                dropped = drop;
                JsonObject entry = new JsonObject();
                entry.add("stack", ItemStack.CODEC.encodeStart(json, drop).getOrThrow());
                entry.add("tooltip_owner", tooltip(drop, owner));
                entry.add("tooltip_other", tooltip(drop, other));
                array.add(entry);
            }
            root.add("drops", array);
            if (dropped != null) {
                helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
                helper.useOn(FLOOR, dropped.copy(), other, Direction.UP);
                root.add("replaced_by_other", placed());
                helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
                helper.useOn(FLOOR, new ItemStack(spec.item.get()), other, Direction.UP);
                root.add("fresh_by_other", placed());
                helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
                helper.useOn(FLOOR, dropped.copy(), owner, Direction.UP);
                root.add("replaced_by_owner", placed());
                helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
            }
            root.add("clicks", clicks(mode));
            root.add("menu", menu(tile));
            root.add("creative", creative(mode));
            return root;
        }

        /// Quebrar em criativo: `ServerPlayerGameMode.handleBlockBreakAction` destrói direto (`destroyAndAck`, sem `getDestroyProgress`), e quem recusa é o `BreakBlockEvent` de
        /// `CommonWorldTickHandler.onBlockBreak`. Um criativo que é o dono e um que não é tentam quebrar a máquina posta pelo dono no `mode`.
        private JsonObject creative(SecurityMode mode) {
            JsonObject out = new JsonObject();
            ServerLevel level = helper.getLevel();
            BlockPos abs = helper.absolutePos(SPOT);
            ServerPlayer stranger = (ServerPlayer) helper.makeTickingMockServerPlayerInLevel(GameType.CREATIVE);
            List<Object[]> players = new ArrayList<>();
            players.add(new Object[]{"other", stranger});
            if (this.owner instanceof ServerPlayer placer) {
                players.add(new Object[]{"owner", placer});
            }
            for (Object[] who : players) {
                helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
                helper.useOn(FLOOR, new ItemStack(spec.item.get()), this.owner, Direction.UP);
                tile().getSecurity().setMode(mode);
                ServerPlayer player = (ServerPlayer) who[1];
                player.setPos(abs.getX() + 0.5, abs.getY(), abs.getZ() + 1.5);
                player.gameMode.handleBlockBreakAction(abs, ServerboundPlayerActionPacket.Action.START_DESTROY_BLOCK, Direction.UP, level.getMaxY(), 1);
                out.addProperty((String) who[0] + "_creative_breaks", level.getBlockState(abs).isAir());
            }
            helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
            return out;
        }

        /// O que o menu sincroniza da segurança (`addContainerTrackers`): a frequência de segurança (`SyncableStreamCodec.frequency`: o `hashCode` do valor e os bytes que
        /// `getPropertyData` escreve) e o modo (`SyncableEnum`, o ordinal).
        private JsonObject menu(TileEntityMekanism tile) {
            SecurityFrequency frequency = tile.getSecurity().getFrequency();
            SyncableStreamCodec<SecurityFrequency> sync = SyncableStreamCodec.frequency(FrequencyTypes.SECURITY, () -> frequency, value -> {
            });
            JsonObject out = new JsonObject();
            out.addProperty("hash", frequency == null ? 0 : frequency.hashCode());
            try {
                Object property = sync.getPropertyData(registries, (short) 0, ISyncableData.DirtyType.DIRTY);
                Field field = property.getClass().getDeclaredField("value");
                field.setAccessible(true);
                StringBuilder hex = new StringBuilder();
                for (byte b : (byte[]) field.get(property)) {
                    hex.append(String.format("%02x", b));
                }
                out.addProperty("bytes", hex.toString());
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
            out.addProperty("mode", tile.getSecurity().getMode().ordinal());
            out.addProperty("owner_name", frequency == null ? "" : frequency.getOwnerName());
            return out;
        }

        /// O dono e o modo do tile que acabou de ser posto.
        private JsonObject placed() {
            TileEntityMekanism tile = tile();
            JsonObject out = new JsonObject();
            CompoundTag tag = tile.saveWithFullMetadata(registries);
            out.add("nbt", NbtOps.INSTANCE.convertTo(JsonOps.INSTANCE, tag));
            out.addProperty("owner", String.valueOf(tile.getSecurity().getOwnerUUID()));
            out.addProperty("mode", tile.getSecurity().getMode().name());
            return out;
        }

        /// A aba: o clique esquerdo (`incrementSecurityMode`) e o direito (`decrementSecurityMode`) do dono e de quem não é, a partir de cada modo.
        private JsonObject clicks(SecurityMode start) {
            helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
            helper.useOn(FLOOR, new ItemStack(spec.item.get()), owner, Direction.UP);
            TileEntityMekanism tile = tile();
            ServerLevel level = helper.getLevel();
            BlockPos abs = helper.absolutePos(SPOT);
            ISecurityObject security = IBlockSecurityUtils.INSTANCE.securityCapability(level, abs, tile);
            JsonObject out = new JsonObject();
            tile.getSecurity().setMode(start);
            SecurityUtils.get().incrementSecurityMode(owner, security, null);
            out.addProperty("owner_next", tile.getSecurity().getMode().name());
            tile.getSecurity().setMode(start);
            SecurityUtils.get().decrementSecurityMode(owner, security, null);
            out.addProperty("owner_previous", tile.getSecurity().getMode().name());
            tile.getSecurity().setMode(start);
            SecurityUtils.get().incrementSecurityMode(other, security, null);
            out.addProperty("other_next", tile.getSecurity().getMode().name());
            tile.getSecurity().setMode(start);
            SecurityUtils.get().decrementSecurityMode(other, security, null);
            out.addProperty("other_previous", tile.getSecurity().getMode().name());
            out.addProperty("owner_matches", security.ownerMatches(owner));
            out.addProperty("other_matches", security.ownerMatches(other));
            helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
            return out;
        }

        private JsonArray tooltip(ItemStack stack, Player viewer) {
            JsonArray tooltip = new JsonArray();
            for (Component text : stack.getTooltipLines(Item.TooltipContext.of(helper.getLevel()), viewer, TooltipFlag.NORMAL)) {
                JsonObject lineJson = new JsonObject();
                lineJson.addProperty("text", text.getString());
                lineJson.add("component", component(text));
                tooltip.add(lineJson);
            }
            return tooltip;
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
