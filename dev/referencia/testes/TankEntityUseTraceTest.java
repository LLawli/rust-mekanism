package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.UUID;
import mekanism.api.security.SecurityMode;
import mekanism.common.capabilities.Capabilities;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismDataComponents;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.util.ItemAccessUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.stats.Stats;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.AgeableMob;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.animal.goat.Goat;
import net.minecraft.world.entity.animal.wolf.Wolf;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.LayeredCauldronBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.NeoForgeMod;
import net.neoforged.neoforge.transfer.ResourceHandler;
import net.neoforged.neoforge.transfer.access.ItemAccess;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// O item do Fluid Tank no modo balde usado numa entidade (`Player.interactOn`, que chama `ItemBlockFluidTank.interactLivingEntity` depois do `interact` da entidade) e
/// o que o servidor conta ao usá-lo (`ServerPlayerGameMode.useItemOn` num caldeirão e `useItem` no mundo): o resultado, a pilha da mão, o dono e as estatísticas.
@ForEachTest(groups = "trace")
public class TankEntityUseTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final String COMMIT = "b2d04e166";
    private static final String DATE = "2026-11-21";

    private static Fluid fluidOf(String name) {
        return switch (name) {
            case "water" -> Fluids.WATER;
            case "lava" -> Fluids.LAVA;
            case "milk" -> NeoForgeMod.MILK.get();
            default -> throw new IllegalArgumentException(name);
        };
    }

    /// `tier:fluido:mB:quantidade`.
    private static ItemStack stackOf(String spec) {
        String[] part = spec.split(":");
        Item item = (switch (part[0]) {
            case "basic" -> MekanismBlocks.BASIC_FLUID_TANK;
            case "advanced" -> MekanismBlocks.ADVANCED_FLUID_TANK;
            case "creative" -> MekanismBlocks.CREATIVE_FLUID_TANK;
            default -> throw new IllegalArgumentException(part[0]);
        }).asItem();
        ItemAccess access = ItemAccessUtils.sideEffectFreeAccess(ItemResource.of(item));
        long wanted = Long.parseLong(part[2]);
        if (wanted > 0) {
            FluidResource resource = FluidResource.of(fluidOf(part[1]));
            long stored = 0;
            for (int i = 0; i < 100_000 && stored < wanted; i++) {
                ResourceHandler<FluidResource> handler = Capabilities.FLUID.getCapability(access);
                int moved;
                try (Transaction transaction = Transaction.openRoot()) {
                    moved = handler.insert(resource, (int) Math.min(Integer.MAX_VALUE, wanted - stored), transaction);
                    transaction.commit();
                }
                if (moved <= 0) {
                    break;
                }
                stored += moved;
            }
        }
        ItemStack stack = ItemAccessUtils.asStack(access);
        stack.setCount(Integer.parseInt(part[3]));
        return stack;
    }

    /// `[item, quantidade, fluido, mB, dono: "", "me" ou "other"]`.
    private static JsonArray stackJson(ItemStack stack, UUID me) {
        JsonArray array = new JsonArray();
        array.add(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).toString());
        array.add(stack.getCount());
        String fluid = "";
        long amount = 0;
        String owner = "";
        if (!stack.isEmpty()) {
            ItemAccess access = ItemAccessUtils.sideEffectFreeAccess(ItemResource.of(stack));
            ResourceHandler<FluidResource> fluids = Capabilities.FLUID.getCapability(access);
            if (fluids != null && fluids.size() > 0 && !fluids.getResource(0).isEmpty()) {
                fluid = fluids.getResource(0).typeHolder().getRegisteredName();
                amount = fluids.getAmountAsLong(0);
            }
            UUID uuid = stack.get(MekanismDataComponents.OWNER);
            owner = uuid == null ? "" : uuid.equals(me) ? "me" : "other";
        }
        array.add(fluid);
        array.add(amount);
        array.add(owner);
        return array;
    }

    private static String name(InteractionResult result) {
        if (result instanceof InteractionResult.Success success) {
            return "success:" + success.swingSource().name().toLowerCase() + (success.heldItemTransformedTo() == null ? "" : ":transformed");
        }
        return result instanceof InteractionResult.Fail ? "fail" : result instanceof InteractionResult.Pass ? "pass" : result.toString();
    }

    private static JsonArray others(ServerPlayer player) {
        JsonArray inventory = new JsonArray();
        int selected = player.getInventory().getSelectedSlot();
        for (int slot = 0; slot < player.getInventory().getContainerSize(); slot++) {
            ItemStack other = player.getInventory().getItem(slot);
            if (slot != selected && !other.isEmpty()) {
                JsonArray entry = new JsonArray();
                entry.add(slot);
                entry.addAll(stackJson(other, player.getUUID()));
                inventory.add(entry);
            }
        }
        return inventory;
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank_entity")
    @TestHolder(description = "The fluid tank item in bucket mode used on entities and what the server counts for its uses.")
    public static void fluidItemEntityUse(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(5, 6, 5).fill(0, 0, 0, 4, 0, 4, Blocks.STONE.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            List<String> lines = new ArrayList<>();
            JsonObject meta = new JsonObject();
            meta.addProperty("commit", COMMIT);
            meta.addProperty("date", DATE);
            meta.addProperty("file", "tank_fluid_item_entity.jsonl");
            meta.addProperty("scenario", "The fluid tank item used on an entity through Player.interactOn, one step per line, all in one tick. step: name; entity: cow, baby_cow, "
                  + "mooshroom, goat, screaming_goat, sheep, wolf (tamed by the player, so its own interact takes the click); stack: tier:fluid:mB:count; mode: mekanism:bucket_mode; "
                  + "creative: the game mode; owner: who owns the item before the step ('' nobody, me, other: another player, with the security mode private). first and second: the "
                  + "results of two uses in a row (success:<swing source>[:transformed], fail, pass); hand1 and hand2: the main hand after each, as [item, count, fluid, mB, owner]; "
                  + "inventory: the other non-empty slots after the second, [slot, ...]; used: how much Stats.ITEM_USED of the item grew over the two uses. Then the lines with "
                  + "'server': what ServerPlayerGameMode counts, useItemOn on a cauldron at (2,1,2) (block: cauldron, water_cauldron (level 3) or lava_cauldron) and useItem on a "
                  + "water source there: result, hand, after (the block), and the growth of [ITEM_USED of the item, FILL_CAULDRON, USE_CAULDRON].");
            JsonObject root = new JsonObject();
            root.add("meta", meta);
            lines.add(root.toString());
            ServerLevel level = helper.getLevel();
            ServerPlayer survival = (ServerPlayer) helper.makeTickingMockServerPlayerInLevel(GameType.SURVIVAL);
            ServerPlayer creative = (ServerPlayer) helper.makeTickingMockServerPlayerInLevel(GameType.CREATIVE);
            BlockPos target = new BlockPos(2, 1, 2);
            BlockPos absolute = helper.absolutePos(target);
            // nome, entidade, pilha, modo balde, criativo, dono
            String[][] steps = {
                  {"claim_then_milk_cow", "cow", "basic:water:0:1", "true", "false", ""},
                  {"milk_cow_owned", "cow", "basic:water:0:1", "true", "false", "me"},
                  {"milk_mooshroom", "mooshroom", "basic:water:0:1", "true", "false", "me"},
                  {"milk_goat", "goat", "basic:water:0:1", "true", "false", "me"},
                  {"milk_screaming_goat", "screaming_goat", "basic:water:0:1", "true", "false", "me"},
                  {"milk_into_milk", "cow", "basic:milk:5000:1", "true", "false", "me"},
                  {"milk_into_water", "cow", "basic:water:5000:1", "true", "false", "me"},
                  {"milk_nearly_full", "cow", "basic:milk:31500:1", "true", "false", "me"},
                  {"milk_full", "cow", "basic:milk:32000:1", "true", "false", "me"},
                  {"milk_stack_of_2", "cow", "basic:water:0:2", "true", "false", "me"},
                  {"milk_creative_item", "cow", "creative:water:0:1", "true", "false", "me"},
                  {"baby_cow", "baby_cow", "basic:water:0:1", "true", "false", ""},
                  {"sheep", "sheep", "basic:water:0:1", "true", "false", ""},
                  {"sheep_owned", "sheep", "basic:water:0:1", "true", "false", "me"},
                  {"mode_off_cow", "cow", "basic:water:0:1", "false", "false", ""},
                  {"other_owner_cow", "cow", "basic:water:0:1", "true", "false", "other"},
                  {"wolf_takes_the_click", "wolf", "basic:water:0:1", "true", "false", ""},
                  {"creative_claim_cow", "cow", "basic:water:0:1", "true", "true", ""},
                  {"creative_milk_cow_owned", "cow", "basic:water:0:1", "true", "true", "me"},
                  {"creative_sheep", "sheep", "basic:water:0:1", "true", "true", ""},
            };
            for (String[] step : steps) {
                ServerPlayer player = Boolean.parseBoolean(step[4]) ? creative : survival;
                player.getInventory().clearContent();
                player.snapTo(absolute.getX() + 0.5, absolute.getY(), absolute.getZ() + 1.5, 180, 0);
                player.setShiftKeyDown(false);
                EntityType<? extends Entity> type = switch (step[1]) {
                    case "cow", "baby_cow" -> EntityTypes.COW;
                    case "mooshroom" -> EntityTypes.MOOSHROOM;
                    case "goat", "screaming_goat" -> EntityTypes.GOAT;
                    case "sheep" -> EntityTypes.SHEEP;
                    case "wolf" -> EntityTypes.WOLF;
                    default -> throw new IllegalArgumentException(step[1]);
                };
                Entity entity = helper.spawn(type, target);
                if (entity instanceof AgeableMob mob) {
                    mob.setBaby(step[1].equals("baby_cow"));
                }
                if (entity instanceof Goat goat) {
                    goat.setScreamingGoat(step[1].equals("screaming_goat"));
                }
                if (entity instanceof Wolf wolf) {
                    wolf.tame(player);
                }
                ItemStack stack = stackOf(step[2]);
                stack.set(MekanismDataComponents.BUCKET_MODE, Boolean.parseBoolean(step[3]));
                if (step[5].equals("me")) {
                    stack.set(MekanismDataComponents.OWNER, player.getUUID());
                } else if (step[5].equals("other")) {
                    stack.set(MekanismDataComponents.OWNER, new UUID(7, 7));
                    stack.set(MekanismDataComponents.SECURITY, SecurityMode.PRIVATE);
                }
                Item item = stack.getItem();
                player.setItemInHand(InteractionHand.MAIN_HAND, stack);
                int usedBefore = player.getStats().getValue(Stats.ITEM_USED.get(item));
                InteractionResult first = player.interactOn(entity, InteractionHand.MAIN_HAND, Vec3.ZERO);
                JsonArray hand1 = stackJson(player.getItemInHand(InteractionHand.MAIN_HAND), player.getUUID());
                InteractionResult second = player.interactOn(entity, InteractionHand.MAIN_HAND, Vec3.ZERO);
                JsonObject line = new JsonObject();
                line.addProperty("step", step[0]);
                line.addProperty("entity", step[1]);
                line.addProperty("stack", step[2]);
                line.addProperty("mode", Boolean.parseBoolean(step[3]));
                line.addProperty("creative", Boolean.parseBoolean(step[4]));
                line.addProperty("owner", step[5]);
                line.addProperty("first", name(first));
                line.add("hand1", hand1);
                line.addProperty("second", name(second));
                line.add("hand2", stackJson(player.getItemInHand(InteractionHand.MAIN_HAND), player.getUUID()));
                line.add("inventory", others(player));
                line.addProperty("used", player.getStats().getValue(Stats.ITEM_USED.get(item)) - usedBefore);
                lines.add(line.toString());
                entity.discard();
            }
            // nome, bloco, pilha, criativo, agachado, função
            String[][] server = {
                  {"cauldron_fill", "cauldron", "basic:water:5000:1", "false", "false", "on"},
                  {"cauldron_fill_lava", "cauldron", "basic:lava:5000:1", "false", "false", "on"},
                  {"cauldron_take", "water_cauldron", "basic:water:0:1", "false", "false", "on"},
                  {"cauldron_take_lava", "lava_cauldron", "basic:water:0:1", "false", "false", "on"},
                  {"cauldron_nothing", "cauldron", "basic:water:0:1", "false", "false", "on"},
                  {"cauldron_creative_fill", "cauldron", "basic:water:5000:1", "true", "false", "on"},
                  {"cauldron_creative_take", "water_cauldron", "basic:water:0:1", "true", "false", "on"},
                  {"use_pickup", "water", "basic:water:0:1", "false", "false", "use"},
                  {"use_place", "air", "basic:water:5000:1", "false", "true", "use"},
                  {"use_place_milk", "air", "basic:milk:5000:1", "false", "true", "use"},
                  {"use_nothing", "air", "basic:water:0:1", "false", "false", "use"},
            };
            for (String[] step : server) {
                ServerPlayer player = Boolean.parseBoolean(step[3]) ? creative : survival;
                player.getInventory().clearContent();
                player.snapTo(absolute.getX() + 0.5, absolute.getY() + 2.2 - player.getEyeHeight(), absolute.getZ() + 0.5, 0, 90);
                player.setShiftKeyDown(Boolean.parseBoolean(step[4]));
                helper.setBlock(target, switch (step[1]) {
                    case "cauldron" -> Blocks.CAULDRON.defaultBlockState();
                    case "water_cauldron" -> Blocks.WATER_CAULDRON.defaultBlockState().setValue(LayeredCauldronBlock.LEVEL, 3);
                    case "lava_cauldron" -> Blocks.LAVA_CAULDRON.defaultBlockState();
                    case "water" -> Blocks.WATER.defaultBlockState();
                    default -> Blocks.AIR.defaultBlockState();
                });
                ItemStack stack = stackOf(step[2]);
                stack.set(MekanismDataComponents.BUCKET_MODE, true);
                stack.set(MekanismDataComponents.OWNER, player.getUUID());
                Item item = stack.getItem();
                player.setItemInHand(InteractionHand.MAIN_HAND, stack);
                int used = player.getStats().getValue(Stats.ITEM_USED.get(item));
                int fill = player.getStats().getValue(Stats.CUSTOM.get(Stats.FILL_CAULDRON));
                int use = player.getStats().getValue(Stats.CUSTOM.get(Stats.USE_CAULDRON));
                InteractionResult result;
                if (step[5].equals("on")) {
                    result = player.gameMode.useItemOn(player, level, stack, InteractionHand.MAIN_HAND,
                          new BlockHitResult(Vec3.atCenterOf(absolute), Direction.UP, absolute, false));
                } else {
                    result = player.gameMode.useItem(player, level, stack, InteractionHand.MAIN_HAND);
                }
                JsonObject line = new JsonObject();
                line.addProperty("server", step[0]);
                line.addProperty("block", step[1]);
                line.addProperty("stack", step[2]);
                line.addProperty("creative", Boolean.parseBoolean(step[3]));
                line.addProperty("sneak", Boolean.parseBoolean(step[4]));
                line.addProperty("result", name(result));
                line.add("hand", stackJson(player.getItemInHand(InteractionHand.MAIN_HAND), player.getUUID()));
                line.add("inventory", others(player));
                BlockState after = level.getBlockState(absolute);
                line.addProperty("after", BuiltInRegistries.BLOCK.getKey(after.getBlock()).toString()
                                          + (after.hasProperty(LayeredCauldronBlock.LEVEL) ? "[level=" + after.getValue(LayeredCauldronBlock.LEVEL) + "]" : ""));
                JsonArray counted = new JsonArray();
                counted.add(player.getStats().getValue(Stats.ITEM_USED.get(item)) - used);
                counted.add(player.getStats().getValue(Stats.CUSTOM.get(Stats.FILL_CAULDRON)) - fill);
                counted.add(player.getStats().getValue(Stats.CUSTOM.get(Stats.USE_CAULDRON)) - use);
                line.add("counted", counted);
                lines.add(line.toString());
            }
            helper.setBlock(target, Blocks.AIR.defaultBlockState());
            survival.setShiftKeyDown(false);
            try {
                Path file = Path.of("trace", "tank_fluid_item_entity.jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }).thenSucceed());
    }
}
