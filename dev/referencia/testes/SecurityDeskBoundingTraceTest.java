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
import java.util.List;
import mekanism.api.security.SecurityMode;
import mekanism.common.item.ItemConfigurator.ConfiguratorMode;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismDataComponents;
import mekanism.common.registries.MekanismItems;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.TileEntitySecurityDesk;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.generators.common.registries.GeneratorsBlocks;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponentMap;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.protocol.Packet;
import net.minecraft.network.protocol.game.ClientboundSystemChatPacket;
import net.minecraft.network.protocol.game.ServerboundPickItemFromBlockPacket;
import net.minecraft.resources.RegistryOps;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.Slot;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.ClipContext;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.CollisionContext;
import net.minecraft.world.phys.shapes.VoxelShape;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.GameTestPlayer;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o bloco de apoio da Security Desk (`BlockBounding`, `TileEntityBoundingBlock`) e os itens dentro da mesa (`MekanismContainer.quickMoveStack`, os dois
/// `SecurityInventorySlot`, o item solto com `mekanism:items`, o Configurator): `security_desk_bounding.jsonl`, uma linha por leitura, na mesma ordem do roteiro do port
/// (`horizon2/port_mesa_final.sh`). A mesa fica voltada para o norte, com o dono 2,5 blocos ao norte dela, como no roteiro do port.
@ForEachTest(groups = "trace")
public class SecurityDeskBoundingTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final BlockPos DESK = new BlockPos(5, 1, 6);
    private static final BlockPos MACHINE = new BlockPos(9, 1, 6);

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_security_desk_bounding")
    @TestHolder(description = "Records the bounding block of the Security Desk and the items inside the desk.")
    public static void bounding(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(12, 8, 12)
              .fill(0, 0, 0, 11, 0, 11, Blocks.STONE.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> {
            Scene scene = new Scene(helper);
            helper.startSequence()
                  .thenExecute(() -> scene.safe("placed", scene::placed))
                  .thenIdle(40)
                  .thenExecute(() -> scene.safe("rest", scene::rest))
                  .thenExecute(() -> scene.safe("crusher_unowned", () -> scene.shift("crusher_unowned", 0)))
                  .thenExecute(() -> scene.safe("stone", () -> scene.shift("stone", 1)))
                  .thenIdle(5)
                  .thenExecute(() -> scene.safe("crusher_after_ticks", () -> scene.read("crusher_after_ticks")))
                  .thenExecute(() -> scene.safe("lock_slot_back", () -> scene.shiftDesk("lock_slot_back", 1)))
                  .thenExecute(() -> scene.safe("crusher_owned", () -> scene.shiftItem("crusher_owned", MekanismBlocks.CRUSHER.asItem())))
                  .thenIdle(5)
                  .thenExecute(() -> scene.safe("crusher_owned_after_ticks", () -> scene.read("crusher_owned_after_ticks")))
                  .thenExecute(() -> scene.safe("unlock_slot_back", () -> scene.shiftDesk("unlock_slot_back", 0)))
                  .thenExecute(() -> scene.safe("heat_unowned", () -> scene.shiftItem("heat_unowned", GeneratorsBlocks.HEAT_GENERATOR.asItem())))
                  .thenIdle(5)
                  .thenExecute(() -> scene.safe("heat_after_ticks", () -> scene.read("heat_after_ticks")))
                  .thenExecute(() -> scene.safe("heat_lock_slot_back", () -> scene.shiftDesk("heat_lock_slot_back", 1)))
                  .thenExecute(() -> scene.safe("heat_owned", () -> scene.shiftItem("heat_owned", GeneratorsBlocks.HEAT_GENERATOR.asItem())))
                  .thenIdle(5)
                  .thenExecute(() -> scene.safe("heat_owned_after_ticks", () -> scene.read("heat_owned_after_ticks")))
                  .thenExecute(() -> scene.safe("heat_unlock_slot_back", () -> scene.shiftDesk("heat_unlock_slot_back", 0)))
                  .thenExecute(() -> scene.safe("crusher_for_drop", () -> scene.shiftItem("crusher_for_drop", MekanismBlocks.CRUSHER.asItem())))
                  .thenIdle(5)
                  .thenExecute(() -> scene.safe("dropped", scene::dropped))
                  .thenIdle(2)
                  .thenExecute(() -> scene.safe("configurator", scene::configurator))
                  .thenIdle(2)
                  .thenExecute(() -> scene.safe("strangers", scene::strangers))
                  .thenExecute(() -> scene.safe("redstone_and_pick", scene::redstoneAndPick))
                  .thenIdle(2)
                  .thenExecute(() -> scene.safe("replaced", scene::replaced))
                  .thenIdle(2)
                  .thenExecute(() -> scene.safe("explosion", scene::explosion))
                  .thenIdle(2)
                  .thenExecute(scene::finish)
                  .thenSucceed();
        });
    }

    private static final class Scene {
        private final MekGameTestHelper helper;
        private final ServerLevel level;
        private final RegistryOps<JsonElement> json;
        private final List<String> lines = new ArrayList<>();
        private final GameTestPlayer owner;
        private final GameTestPlayer stranger;
        private final BlockPos desk;
        private ItemEntity onMonitor;
        private ItemEntity onFront;

        Scene(MekGameTestHelper helper) {
            this.helper = helper;
            this.level = helper.getLevel();
            this.json = RegistryOps.create(JsonOps.INSTANCE, level.registryAccess());
            this.owner = (GameTestPlayer) helper.makeTickingMockServerPlayerInLevel(GameType.CREATIVE);
            this.stranger = (GameTestPlayer) helper.makeTickingMockServerPlayerInLevel(GameType.SURVIVAL);
            this.desk = helper.absolutePos(DESK);
            owner.preventItemPickup();
            stranger.preventItemPickup();
            stand(owner);
            stand(stranger);
        }

        /// O jogador 2,5 blocos ao norte da mesa, olhando para o sul: o bloco que ele põe fica voltado para o norte.
        private void stand(GameTestPlayer player) {
            player.snapTo(desk.getX() + 0.5, desk.getY(), desk.getZ() - 2.5, 0.0F, 0.0F);
        }

        private TileEntitySecurityDesk tile() {
            return level.getBlockEntity(desk) instanceof TileEntitySecurityDesk tile ? tile : null;
        }

        private JsonObject line(String name) {
            JsonObject out = new JsonObject();
            out.addProperty("step", name);
            return out;
        }

        void safe(String name, Runnable stage) {
            try {
                stage.run();
            } catch (RuntimeException ex) {
                JsonObject out = line("exception_in_" + name);
                out.addProperty("exception", ex.toString());
                StackTraceElement[] trace = ex.getStackTrace();
                out.addProperty("at", trace.length == 0 ? "" : trace[0].toString());
                add(out);
            }
        }

        private void add(JsonObject line) {
            lines.add(line.toString());
        }

        private JsonElement nbt(Tag tag) {
            return tag == null ? JsonNull.INSTANCE : NbtOps.INSTANCE.convertTo(JsonOps.INSTANCE, tag);
        }

        private JsonElement tileNbt(BlockPos pos) {
            BlockEntity be = level.getBlockEntity(pos);
            return be == null ? JsonNull.INSTANCE : nbt(be.saveWithFullMetadata(level.registryAccess()));
        }

        private String block(BlockPos pos) {
            return level.getBlockState(pos).toString();
        }

        private JsonElement stack(ItemStack stack) {
            if (stack.isEmpty()) {
                return JsonNull.INSTANCE;
            }
            return ItemStack.CODEC.encodeStart(json, stack).getOrThrow(IllegalStateException::new);
        }

        private JsonArray boxes(VoxelShape shape) {
            JsonArray out = new JsonArray();
            for (AABB box : shape.toAabbs()) {
                JsonArray one = new JsonArray();
                for (double value : new double[]{box.minX, box.minY, box.minZ, box.maxX, box.maxY, box.maxZ}) {
                    one.add(value);
                }
                out.add(one);
            }
            return out;
        }

        private JsonObject clip(double tx, double ty, double tz) {
            Vec3 from = new Vec3(desk.getX() + 0.5, desk.getY() + 1.62, desk.getZ() - 2.5);
            Vec3 to = new Vec3(desk.getX() + tx, desk.getY() + ty, desk.getZ() + tz);
            // O alvo fica 3 blocos além do ponto mirado, na mesma reta.
            Vec3 far = from.add(to.subtract(from).normalize().scale(6));
            BlockHitResult hit = level.clip(new ClipContext(from, far, ClipContext.Block.OUTLINE, ClipContext.Fluid.NONE, CollisionContext.empty()));
            JsonObject out = new JsonObject();
            out.addProperty("type", hit.getType().name());
            out.addProperty("block", block(hit.getBlockPos()));
            out.addProperty("pos", rel(hit.getBlockPos()));
            out.addProperty("face", hit.getDirection().getSerializedName());
            Vec3 at = hit.getLocation();
            out.addProperty("point", (at.x - desk.getX()) + " " + (at.y - desk.getY()) + " " + (at.z - desk.getZ()));
            return out;
        }

        private String rel(BlockPos pos) {
            return (pos.getX() - desk.getX()) + " " + (pos.getY() - desk.getY()) + " " + (pos.getZ() - desk.getZ());
        }

        private BlockHitResult hit(BlockPos pos, Direction face) {
            return new BlockHitResult(Vec3.atCenterOf(pos).relative(face, 0.5), face, pos, false);
        }

        private static JsonElement component(Component component) {
            return ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, component).getOrThrow();
        }

        @SuppressWarnings({"unchecked", "rawtypes"})
        private JsonObject drain(GameTestPlayer player) {
            JsonArray overlay = new JsonArray();
            JsonArray chat = new JsonArray();
            for (Packet<?> packet : (List<Packet<?>>) (List) ((java.util.stream.Stream) player.getOutboundPackets((Class) Packet.class)).toList()) {
                if (packet instanceof ClientboundSystemChatPacket system) {
                    (system.overlay() ? overlay : chat).add(component(system.content()));
                }
            }
            player.clearOutboundPackets();
            JsonObject out = new JsonObject();
            out.add("overlay", overlay);
            out.add("chat", chat);
            return out;
        }

        private String menu(GameTestPlayer player, boolean close) {
            AbstractContainerMenu menu = player.containerMenu;
            if (menu == player.inventoryMenu) {
                return "";
            }
            String name = menu.getClass().getSimpleName();
            if (close) {
                player.closeContainer();
            }
            return name;
        }

        /// Um clique direito de `player` em `pos` com `held` na mão: o resultado, o menu que abriu, as mensagens e o estado da mesa depois.
        private JsonObject click(String name, GameTestPlayer player, ItemStack held, BlockPos pos, Direction face, boolean sneak) {
            JsonObject out = line(name);
            player.setItemInHand(InteractionHand.MAIN_HAND, held);
            player.setShiftKeyDown(sneak);
            player.clearOutboundPackets();
            try {
                InteractionResult result = player.gameMode.useItemOn(player, level, player.getItemInHand(InteractionHand.MAIN_HAND), InteractionHand.MAIN_HAND, hit(pos, face));
                out.addProperty("result", result.getClass().getSimpleName());
                out.addProperty("consumes_action", result.consumesAction());
            } catch (RuntimeException ex) {
                out.addProperty("exception", ex.getClass().getSimpleName() + ": " + ex.getMessage());
            }
            player.setShiftKeyDown(false);
            out.addProperty("menu", menu(player, true));
            out.add("messages", drain(player));
            out.addProperty("desk", block(desk));
            out.addProperty("above", block(desk.above()));
            TileEntitySecurityDesk tile = tile();
            if (tile != null) {
                out.add("slots", slots(tile));
            }
            out.add("entities", entities(false));
            out.add("inventory", inventory(player));
            return out;
        }

        private JsonArray slots(TileEntityMekanism tile) {
            JsonArray out = new JsonArray();
            for (var slot : tile.getInventorySlots()) {
                out.add(stack(slot.asItemStack()));
            }
            return out;
        }

        private JsonArray entities(boolean discard) {
            JsonArray out = new JsonArray();
            for (ItemEntity entity : level.getEntitiesOfClass(ItemEntity.class, new AABB(desk).inflate(8))) {
                if (entity == onMonitor || entity == onFront) {
                    continue;
                }
                JsonObject one = new JsonObject();
                one.add("item", stack(entity.getItem()));
                one.addProperty("pos", (entity.getX() - desk.getX()) + " " + (entity.getY() - desk.getY()) + " " + (entity.getZ() - desk.getZ()));
                out.add(one);
                if (discard) {
                    entity.discard();
                }
            }
            return out;
        }

        private JsonArray inventory(GameTestPlayer player) {
            JsonArray out = new JsonArray();
            for (int i = 0; i < player.getInventory().getContainerSize(); i++) {
                ItemStack stack = player.getInventory().getItem(i);
                if (!stack.isEmpty()) {
                    JsonObject one = new JsonObject();
                    one.addProperty("slot", i);
                    one.add("item", stack(stack));
                    out.add(one);
                }
            }
            return out;
        }

        private void place() {
            stand(owner);
            helper.useOn(DESK.below(), new ItemStack(MekanismBlocks.SECURITY_DESK.asItem()), owner, Direction.UP);
            owner.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
        }

        private ItemEntity item(double x, double y, double z) {
            ItemEntity entity = new ItemEntity(level, desk.getX() + x, desk.getY() + y, desk.getZ() + z, new ItemStack(Items.STONE), 0, 0, 0);
            entity.setNeverPickUp();
            level.addFreshEntity(entity);
            return entity;
        }

        void placed() {
            place();
            JsonObject out = line("placed");
            out.addProperty("owner", owner.getUUID().toString());
            out.addProperty("stranger", stranger.getUUID().toString());
            out.addProperty("desk", block(desk));
            out.addProperty("above", block(desk.above()));
            out.add("above_nbt", tileNbt(desk.above()));
            out.add("above_update_tag", nbt(level.getBlockEntity(desk.above()).getUpdateTag(level.registryAccess())));
            BlockState main = level.getBlockState(desk);
            BlockState bounding = level.getBlockState(desk.above());
            out.add("desk_shape", boxes(main.getShape(level, desk)));
            out.add("desk_collision", boxes(main.getCollisionShape(level, desk)));
            out.add("above_shape", boxes(bounding.getShape(level, desk.above())));
            out.add("above_collision", boxes(bounding.getCollisionShape(level, desk.above())));
            out.addProperty("desk_resistance", main.getExplosionResistance(level, desk, null));
            out.addProperty("above_resistance", bounding.getExplosionResistance(level, desk.above(), null));
            out.addProperty("above_block_resistance", bounding.getBlock().getExplosionResistance());
            out.addProperty("desk_destroy_owner", main.getDestroyProgress(owner, level, desk));
            out.addProperty("above_destroy_owner", bounding.getDestroyProgress(owner, level, desk.above()));
            out.addProperty("desk_destroy_stranger", main.getDestroyProgress(stranger, level, desk));
            out.addProperty("above_destroy_stranger", bounding.getDestroyProgress(stranger, level, desk.above()));
            add(out);

            JsonObject aim = line("aim");
            aim.add("upper", clip(0.5, 1.3, 0.7));
            aim.add("lower", clip(0.5, 0.4, 0.0));
            add(aim);

            add(click("owner_clicks_bounding", owner, ItemStack.EMPTY, desk.above(), Direction.NORTH, false));
            add(click("owner_clicks_desk", owner, ItemStack.EMPTY, desk, Direction.NORTH, false));

            JsonObject pick = line("pick");
            pick.add("above", stack(bounding.getBlock().getCloneItemStack(level, desk.above(), bounding, false, owner)));
            pick.add("above_with_data", stack(bounding.getBlock().getCloneItemStack(level, desk.above(), bounding, true, owner)));
            pick.add("desk", stack(main.getBlock().getCloneItemStack(level, desk, main, false, owner)));
            add(pick);

            onMonitor = item(0.5, 2.5, 0.75);
            onFront = item(0.5, 2.5, 0.2);
        }

        void rest() {
            JsonObject out = line("item_rest");
            out.addProperty("on_monitor_y", onMonitor.getY() - desk.getY());
            out.addProperty("on_monitor_ground", onMonitor.onGround());
            out.addProperty("on_front_y", onFront.getY() - desk.getY());
            out.addProperty("on_front_ground", onFront.onGround());
            add(out);
            onMonitor.discard();
            onFront.discard();
            onMonitor = null;
            onFront = null;
            // A barra do dono: 0 Crusher sem dono, 1 pedra, 2 Heat Generator; e a tela da mesa aberta.
            owner.getInventory().clearContent();
            owner.getInventory().setItem(0, new ItemStack(MekanismBlocks.CRUSHER.asItem()));
            owner.getInventory().setItem(1, new ItemStack(Items.STONE));
            owner.getInventory().setItem(2, new ItemStack(GeneratorsBlocks.HEAT_GENERATOR.asItem()));
            owner.setShiftKeyDown(false);
            owner.gameMode.useItemOn(owner, level, ItemStack.EMPTY, InteractionHand.MAIN_HAND, hit(desk, Direction.NORTH));
            JsonObject opened = line("menu_slots");
            opened.addProperty("menu", menu(owner, false));
            JsonArray order = new JsonArray();
            for (Slot slot : owner.containerMenu.slots) {
                order.add((slot.container == owner.getInventory() ? "inv" : "tile") + ":" + slot.getContainerSlot());
            }
            opened.add("order", order);
            add(opened);
        }

        private int menuIndex(boolean inventory, int containerSlot) {
            List<Slot> slots = owner.containerMenu.slots;
            int tile = 0;
            for (int i = 0; i < slots.size(); i++) {
                boolean inv = slots.get(i).container == owner.getInventory();
                if (inventory && inv && slots.get(i).getContainerSlot() == containerSlot) {
                    return i;
                }
                if (!inventory && !inv) {
                    if (tile == containerSlot) {
                        return i;
                    }
                    tile++;
                }
            }
            return -1;
        }

        private void moved(String name, int index) {
            JsonObject out = line("shift_" + name);
            out.addProperty("menu_index", index);
            if (index >= 0) {
                // `AbstractContainerMenu.doClick` com `QUICK_MOVE`: repete enquanto o slot devolve o mesmo item.
                Slot slot = owner.containerMenu.slots.get(index);
                ItemStack moved = owner.containerMenu.quickMoveStack(owner, index);
                int rounds = 1;
                while (!moved.isEmpty() && ItemStack.isSameItem(slot.getItem(), moved) && rounds < 8) {
                    moved = owner.containerMenu.quickMoveStack(owner, index);
                    rounds++;
                }
                out.addProperty("rounds", rounds);
            }
            out.add("slots", slots(tile()));
            out.add("inventory", inventory(owner));
            add(out);
        }

        void shift(String name, int inventorySlot) {
            moved(name, menuIndex(true, inventorySlot));
        }

        void shiftDesk(String name, int deskSlot) {
            moved(name, menuIndex(false, deskSlot));
        }

        void shiftItem(String name, net.minecraft.world.item.Item item) {
            int found = -1;
            for (int i = 0; i < owner.getInventory().getContainerSize(); i++) {
                if (owner.getInventory().getItem(i).is(item)) {
                    found = i;
                    break;
                }
            }
            moved(name, found < 0 ? -1 : menuIndex(true, found));
        }

        void read(String name) {
            JsonObject out = line(name);
            out.add("slots", slots(tile()));
            add(out);
        }

        void dropped() {
            owner.closeContainer();
            TileEntitySecurityDesk tile = tile();
            JsonObject out = line("desk_with_locked_crusher");
            out.add("slots", slots(tile));
            out.add("desk_nbt", tileNbt(desk));
            DataComponentMap components = tile.collectComponents();
            out.add("components", DataComponentMap.CODEC.encodeStart(json, components).getOrThrow(IllegalStateException::new));
            BlockState state = level.getBlockState(desk);
            JsonArray drops = new JsonArray();
            List<ItemStack> stacks = Block.getDrops(state, level, desk, tile, owner, ItemStack.EMPTY);
            for (ItemStack drop : stacks) {
                drops.add(stack(drop));
            }
            out.add("drops", drops);
            add(out);

            // O dono quebra a mesa em sobrevivência: o que cai, e o que sobra dos dois blocos.
            owner.getInventory().clearContent();
            owner.gameMode.changeGameModeForPlayer(GameType.SURVIVAL);
            owner.setItemInHand(InteractionHand.MAIN_HAND, new ItemStack(Items.NETHERITE_PICKAXE));
            JsonObject broken = line("broken_in_survival");
            broken.addProperty("destroyed", owner.gameMode.destroyBlock(desk));
            broken.addProperty("desk", block(desk));
            broken.addProperty("above", block(desk.above()));
            broken.add("entities", entities(true));
            add(broken);
            owner.gameMode.changeGameModeForPlayer(GameType.CREATIVE);

            // O item solto recolocado: os dois slots voltam ao tile.
            stand(owner);
            ItemStack item = stacks.isEmpty() ? new ItemStack(MekanismBlocks.SECURITY_DESK.asItem()) : stacks.getFirst().copy();
            helper.useOn(DESK.below(), item, owner, Direction.UP);
            owner.getInventory().clearContent();
            JsonObject back = line("placed_again");
            back.addProperty("desk", block(desk));
            back.add("slots", tile() == null ? JsonNull.INSTANCE : slots(tile()));
            back.add("desk_nbt", tileNbt(desk));
            add(back);
        }

        private ItemStack configurator(ConfiguratorMode mode) {
            ItemStack stack = new ItemStack(MekanismItems.CONFIGURATOR.asItem());
            stack.set(MekanismDataComponents.CONFIGURATOR_MODE, mode);
            return stack;
        }

        void configurator() {
            stand(owner);
            for (int i = 1; i <= 4; i++) {
                add(click("rotate_" + i, owner, configurator(ConfiguratorMode.ROTATE), desk, Direction.NORTH, false));
            }
            JsonObject viaBounding = click("rotate_by_bounding", owner, configurator(ConfiguratorMode.ROTATE), desk.above(), Direction.NORTH, false);
            viaBounding.add("above_nbt", tileNbt(desk.above()));
            add(viaBounding);
            add(click("rotate_sneak_west", owner, configurator(ConfiguratorMode.ROTATE), desk, Direction.WEST, true));
            add(click("rotate_sneak_west_again", owner, configurator(ConfiguratorMode.ROTATE), desk, Direction.WEST, true));
            add(click("rotate_sneak_up", owner, configurator(ConfiguratorMode.ROTATE), desk, Direction.UP, true));
            add(click("rotate_sneak_by_bounding_west", owner, configurator(ConfiguratorMode.ROTATE), desk.above(), Direction.WEST, true));
            add(click("items_no_sneak", owner, configurator(ConfiguratorMode.CONFIGURATE_ITEMS), desk, Direction.NORTH, false));
            add(click("items_sneak", owner, configurator(ConfiguratorMode.CONFIGURATE_ITEMS), desk, Direction.NORTH, true));
            add(click("empty_no_sneak", owner, configurator(ConfiguratorMode.EMPTY), desk, Direction.NORTH, false));
            add(click("empty_sneak", owner, configurator(ConfiguratorMode.EMPTY), desk, Direction.NORTH, true));
            JsonObject after = line("after_empty");
            after.add("entities", entities(true));
            add(after);
        }

        void strangers() {
            stand(stranger);
            add(click("stranger_clicks_bounding", stranger, ItemStack.EMPTY, desk.above(), Direction.NORTH, false));
            add(click("stranger_clicks_desk", stranger, ItemStack.EMPTY, desk, Direction.NORTH, false));
            add(click("stranger_rotate", stranger, configurator(ConfiguratorMode.ROTATE), desk, Direction.NORTH, false));
            add(click("stranger_rotate_by_bounding", stranger, configurator(ConfiguratorMode.ROTATE), desk.above(), Direction.NORTH, false));
            add(click("stranger_rotate_sneak", stranger, configurator(ConfiguratorMode.ROTATE), desk, Direction.WEST, true));
            add(click("stranger_items_sneak", stranger, configurator(ConfiguratorMode.CONFIGURATE_ITEMS), desk, Direction.NORTH, true));
            add(click("stranger_empty_sneak", stranger, configurator(ConfiguratorMode.EMPTY), desk, Direction.NORTH, true));
            add(click("stranger_wrench_sneak", stranger, configurator(ConfiguratorMode.WRENCH), desk, Direction.NORTH, true));
            // Uma máquina `PRIVATE` do dono: o Configurator do estranho em `ROTATE`, sem agachar.
            BlockPos machine = helper.absolutePos(MACHINE);
            owner.snapTo(machine.getX() + 0.5, machine.getY(), machine.getZ() - 2.5, 0.0F, 0.0F);
            helper.useOn(MACHINE.below(), new ItemStack(MekanismBlocks.CRUSHER.asItem()), owner, Direction.UP);
            owner.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
            if (level.getBlockEntity(machine) instanceof TileEntityMekanism tile) {
                tile.getSecurity().setMode(SecurityMode.PRIVATE);
                for (String who : new String[]{"stranger", "owner"}) {
                    GameTestPlayer player = who.equals("stranger") ? stranger : owner;
                    JsonObject out = line(who + "_rotates_private_machine");
                    out.addProperty("before", block(machine));
                    player.setItemInHand(InteractionHand.MAIN_HAND, configurator(ConfiguratorMode.ROTATE));
                    player.clearOutboundPackets();
                    InteractionResult result = player.gameMode.useItemOn(player, level, player.getItemInHand(InteractionHand.MAIN_HAND), InteractionHand.MAIN_HAND,
                          hit(machine, Direction.NORTH));
                    out.addProperty("result", result.getClass().getSimpleName());
                    if (result instanceof InteractionResult.Success success) {
                        out.addProperty("swing_source", success.swingSource().name());
                    }
                    out.addProperty("menu", menu(player, true));
                    out.add("messages", drain(player));
                    out.addProperty("after", block(machine));
                    add(out);
                }
            }
            stand(owner);
            // O desmonte, por último: o dono agachado com `WRENCH`.
            JsonObject wrench = click("wrench_sneak", owner, configurator(ConfiguratorMode.WRENCH), desk, Direction.NORTH, true);
            add(wrench);
            JsonObject after = line("after_wrench");
            after.add("entities", entities(true));
            add(after);
            owner.getInventory().clearContent();
        }

        /// O `redstone` do tile do apoio (`TileEntityBoundingBlock.onNeighborChange`) e o Ctrl + pick no apoio pelo pacote de verdade
        /// (`handlePickItemFromBlock`, que chama `addBlockDataToItem` com a posição mirada).
        void redstoneAndPick() {
            // A mesa foi desmontada no fim de `strangers`: posta de novo pelo dono.
            level.setBlock(desk, Blocks.AIR.defaultBlockState(), 3);
            level.setBlock(desk.above(), Blocks.AIR.defaultBlockState(), 3);
            entities(true);
            place();
            BlockPos side = desk.above().east();
            JsonObject out = line("bounding_redstone");
            out.add("before_nbt", tileNbt(desk.above()));
            level.setBlock(side, Blocks.REDSTONE_BLOCK.defaultBlockState(), 3);
            out.add("powered_nbt", tileNbt(desk.above()));
            out.add("powered_update_tag", nbt(level.getBlockEntity(desk.above()).getUpdateTag(level.registryAccess())));
            out.addProperty("powered_chunk_unsaved", level.getChunkAt(desk.above()).isUnsaved());
            level.setBlock(side, Blocks.AIR.defaultBlockState(), 3);
            out.add("unpowered_nbt", tileNbt(desk.above()));
            add(out);

            stand(owner);
            JsonObject pick = line("ctrl_pick_bounding");
            for (boolean data : new boolean[]{false, true}) {
                owner.getInventory().clearContent();
                owner.getInventory().setSelectedSlot(0);
                level.setBlock(side, data ? Blocks.REDSTONE_BLOCK.defaultBlockState() : Blocks.AIR.defaultBlockState(), 3);
                owner.connection.handlePickItemFromBlock(new ServerboundPickItemFromBlockPacket(desk.above(), data));
                pick.add(data ? "with_data" : "without_data", stack(owner.getInventory().getSelectedItem()));
            }
            owner.getInventory().clearContent();
            owner.connection.handlePickItemFromBlock(new ServerboundPickItemFromBlockPacket(desk, true));
            pick.add("desk_with_data", stack(owner.getInventory().getSelectedItem()));
            owner.getInventory().clearContent();
            level.setBlock(side, Blocks.AIR.defaultBlockState(), 3);
            add(pick);
        }

        void replaced() {
            for (BlockState with : new BlockState[]{Blocks.AIR.defaultBlockState(), Blocks.STONE.defaultBlockState()}) {
                level.setBlock(desk, Blocks.AIR.defaultBlockState(), 3);
                level.setBlock(desk.above(), Blocks.AIR.defaultBlockState(), 3);
                entities(true);
                place();
                JsonObject out = line("bounding_replaced_by_" + BuiltInRegistries.BLOCK.getKey(with.getBlock()).getPath());
                out.addProperty("desk_before", block(desk));
                level.setBlock(desk.above(), with, 3);
                out.addProperty("desk", block(desk));
                out.addProperty("above", block(desk.above()));
                out.add("entities", entities(true));
                add(out);
            }
            level.setBlock(desk, Blocks.AIR.defaultBlockState(), 3);
            level.setBlock(desk.above(), Blocks.AIR.defaultBlockState(), 3);
            place();
        }

        void explosion() {
            // A TNT do roteiro do port: 2 blocos a leste, na altura do apoio (`PrimedTnt.explode`: `getY(0.0625)`), força 4.
            JsonObject out = line("explosion_east_of_bounding");
            out.addProperty("desk_before", block(desk));
            level.explode(null, desk.getX() + 2.5, desk.getY() + 1.0625, desk.getZ() + 0.5, 4.0F, Level.ExplosionInteraction.TNT);
            out.addProperty("desk", block(desk));
            out.addProperty("above", block(desk.above()));
            out.addProperty("floor_under_tnt", block(desk.offset(2, -1, 0)));
            JsonArray desks = new JsonArray();
            for (ItemEntity entity : level.getEntitiesOfClass(ItemEntity.class, new AABB(desk).inflate(10))) {
                if (entity.getItem().is(MekanismBlocks.SECURITY_DESK.asItem())) {
                    desks.add(stack(entity.getItem()));
                }
                entity.discard();
            }
            out.add("desk_items", desks);
            add(out);
        }

        void finish() {
            try {
                Path file = Path.of("trace", "security_desk_bounding.jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }
}
