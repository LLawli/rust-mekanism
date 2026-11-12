package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonPrimitive;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.common.block.states.BlockStateHelper;
import mekanism.common.block.states.FluidLogType;
import mekanism.common.block.states.IStateFluidLoggable;
import mekanism.common.tests.helpers.MekGameTestHelper;
import net.minecraft.SharedConstants;
import net.minecraft.commands.arguments.EntityAnchorArgument;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.tags.FluidTags;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.animal.pig.Pig;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o que o original faz com a água e a lava dentro de um bloco `fluid_logged` (`IStateFluidLoggable`), para `dev/referencia/horizon2/h2_fluido_no_bloco.py` repetir em jogo:
///
/// - `fluido_no_bloco_flow.jsonl`: o fluido posto por `placeLiquid` (a rota do balde) em oito blocos, tick a tick. Cada linha é um tick em que algo mudou no volume de um cenário:
///   `cells` leva `[x, y, z, bloco com as propriedades, fluido, quantidade]` das células que mudaram.
/// - `fluido_no_bloco_entity.jsonl`: jogador, porco e item soltos na célula do cabo com água, com lava e sem fluido (controle), tick a tick.
/// - `fluido_no_bloco_bucket.jsonl`: os baldes de um jogador falso (sobrevivência e criativo) em blocos vazios, cheios de água e de lava, em pé e agachado, e o que `canPlaceLiquid`
///   e `pickupBlock` respondem sozinhos.
///
/// Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace")
public class FluidoNoBlocoTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final String BATCH = "trace_fluido_no_bloco";
    private static final int FLOW_TICKS = 200;
    private static final int ENTITY_TICKS = 60;
    private static final int CELL = 18;

    private static final String[] BLOCKS = {
          "mekanism:basic_universal_cable", "mekanism:basic_mechanical_pipe", "mekanism:basic_fluid_tank", "mekanism:basic_energy_cube",
          "mekanism:industrial_alarm", "mekanism:chargepad", "mekanism:basic_smelting_factory", "mekanism:personal_chest"};

    // ------------------------------------------------------------------------------------------------------------------------------ json

    private static JsonElement el(Object value) {
        if (value == null) {
            return JsonNull.INSTANCE;
        } else if (value instanceof JsonElement element) {
            return element;
        } else if (value instanceof Boolean bool) {
            return new JsonPrimitive(bool);
        } else if (value instanceof Number number) {
            return new JsonPrimitive(number);
        }
        return new JsonPrimitive(value.toString());
    }

    private static JsonObject obj(Object... keysAndValues) {
        JsonObject object = new JsonObject();
        for (int i = 0; i < keysAndValues.length; i += 2) {
            object.add((String) keysAndValues[i], el(keysAndValues[i + 1]));
        }
        return object;
    }

    private static JsonArray arr(Object... values) {
        JsonArray array = new JsonArray();
        for (Object value : values) {
            array.add(el(value));
        }
        return array;
    }

    private static double rnd(double value) {
        return Math.round(value * 10000.0) / 10000.0;
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

    private static Block block(String id) {
        return BuiltInRegistries.BLOCK.getValue(Identifier.parse(id));
    }

    private static String describe(BlockState state) {
        String text = state.toString();
        return text.startsWith("Block{") ? text.substring(6).replace("}", "") : text;
    }

    private static String fluidOf(BlockState state) {
        return BuiltInRegistries.FLUID.getKey(state.getFluidState().getType()).toString();
    }

    private static void template(DynamicTest test, int sx, int sy, int sz) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(sx, sy, sz).fill(0, 0, 0, sx - 1, 0, sz - 1, Blocks.STONE.defaultBlockState()));
    }

    /// Põe o fluido pela rota do balde: `IStateFluidLoggable.placeLiquid`, que grava o estado e agenda o tick do fluido.
    private static boolean place(ServerLevel level, BlockPos pos, String blockId, boolean water) {
        BlockState state = block(blockId).defaultBlockState();
        level.setBlock(pos, state, 3);
        state = level.getBlockState(pos);
        return ((IStateFluidLoggable) state.getBlock()).placeLiquid(level, pos, state, (water ? Fluids.WATER : Fluids.LAVA).defaultFluidState());
    }

    // ------------------------------------------------------------------------------------------------------------------------------ flow

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records water and lava placed inside eight Mekanism blocks, spreading tick by tick.")
    public static void fluidoNoBlocoFlow(final DynamicTest test) {
        template(test, CELL * BLOCKS.length, 6, CELL * 2);
        Flow flow = new Flow();
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> flow.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> flow.start(helper))
              .thenExecuteAfter(FLOW_TICKS + 6, flow::write)
              .thenSucceed());
    }

    private static final class Flow {
        ServerLevel level;
        BlockPos origin;
        boolean recording;
        int tick;
        BlockState[][] previous;
        final List<String> lines = new ArrayList<>();

        BlockPos cell(int index, int row) {
            return origin.offset(CELL * index + CELL / 2, 1, CELL * row + CELL / 2);
        }

        void start(MekGameTestHelper helper) {
            level = helper.getLevel();
            origin = helper.absolutePos(BlockPos.ZERO);
            for (int row = 0; row < 2; row++) {
                for (int index = 0; index < BLOCKS.length; index++) {
                    boolean placed = place(level, cell(index, row), BLOCKS[index], row == 0);
                    lines.add(obj("placed", BLOCKS[index], "fluid", row == 0 ? "water" : "lava", "ok", placed).toString());
                }
            }
            previous = snapshot();
            recording = true;
        }

        BlockState[][] snapshot() {
            BlockState[][] all = new BlockState[BLOCKS.length * 2][];
            for (int row = 0; row < 2; row++) {
                for (int index = 0; index < BLOCKS.length; index++) {
                    BlockPos center = cell(index, row);
                    BlockState[] cells = new BlockState[17 * 5 * 17];
                    int i = 0;
                    for (int x = -8; x <= 8; x++) {
                        for (int y = 0; y < 5; y++) {
                            for (int z = -8; z <= 8; z++) {
                                cells[i++] = level.getBlockState(center.offset(x, y - 1, z));
                            }
                        }
                    }
                    all[row * BLOCKS.length + index] = cells;
                }
            }
            return all;
        }

        void record() {
            if (!recording || tick > FLOW_TICKS) {
                return;
            }
            BlockState[][] current = snapshot();
            for (int row = 0; row < 2; row++) {
                for (int index = 0; index < BLOCKS.length; index++) {
                    int scenario = row * BLOCKS.length + index;
                    JsonArray cells = new JsonArray();
                    int i = 0;
                    for (int x = -8; x <= 8; x++) {
                        for (int y = 0; y < 5; y++) {
                            for (int z = -8; z <= 8; z++) {
                                BlockState state = current[scenario][i];
                                boolean changed = tick == 0 ? !state.isAir() : state != previous[scenario][i];
                                if (changed) {
                                    cells.add(arr(x, y - 1, z, describe(state), fluidOf(state), state.getFluidState().getAmount()));
                                }
                                i++;
                            }
                        }
                    }
                    if (!cells.isEmpty()) {
                        lines.add(obj("tick", tick, "block", BLOCKS[index], "fluid", row == 0 ? "water" : "lava", "cells", cells).toString());
                    }
                }
            }
            previous = current;
            tick++;
        }

        void write() {
            recording = false;
            FluidoNoBlocoTraceTest.write("fluido_no_bloco_flow", lines);
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------------ entity

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records a player, a pig and an item inside a cable cell with water, with lava and with nothing.")
    public static void fluidoNoBlocoEntity(final DynamicTest test) {
        template(test, 40, 8, 40);
        Entities scene = new Entities();
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.start(helper))
              .thenExecuteAfter(ENTITY_TICKS + 6, scene::write)
              .thenSucceed());
    }

    private static final class Run {
        final String fluid;
        final String kind;
        final Entity entity;

        Run(String fluid, String kind, Entity entity) {
            this.fluid = fluid;
            this.kind = kind;
            this.entity = entity;
        }
    }

    private static final class Entities {
        ServerLevel level;
        BlockPos origin;
        boolean recording;
        int tick;
        final List<Run> runs = new ArrayList<>();
        final List<String> lines = new ArrayList<>();

        void start(MekGameTestHelper helper) {
            level = helper.getLevel();
            origin = helper.absolutePos(BlockPos.ZERO);
            BlockState stone = Blocks.STONE.defaultBlockState();
            int index = 0;
            for (String fluid : new String[]{"water", "lava", "none"}) {
                for (String kind : new String[]{"player", "pig", "item"}) {
                    // Um poço de 1 por 1 com paredes e teto de pedra (y de 1 a 2) e o cabo na célula de baixo (y = 1).
                    int cx = 3 + 4 * (index % 8);
                    int cz = 3 + 4 * (index / 8);
                    index++;
                    for (int y = 1; y <= 2; y++) {
                        for (int[] d : new int[][]{{1, 0}, {-1, 0}, {0, 1}, {0, -1}}) {
                            level.setBlock(origin.offset(cx + d[0], y, cz + d[1]), stone, 2);
                        }
                    }
                    level.setBlock(origin.offset(cx, 3, cz), stone, 2);
                    BlockPos pos = origin.offset(cx, 1, cz);
                    if (fluid.equals("none")) {
                        level.setBlock(pos, block("mekanism:basic_universal_cable").defaultBlockState(), 3);
                    } else {
                        place(level, pos, "mekanism:basic_universal_cable", fluid.equals("water"));
                    }
                    runs.add(new Run(fluid, kind, spawn(kind, Vec3.atBottomCenterOf(pos).add(0, 0.05, 0), helper)));
                }
            }
            recording = true;
        }

        Entity spawn(String kind, Vec3 at, MekGameTestHelper helper) {
            switch (kind) {
                case "player" -> {
                    ServerPlayer player = helper.makeTickingMockServerPlayerInLevel(GameType.SURVIVAL);
                    level.getServer().getConnection().getConnections().remove(player.connection.getConnection());
                    player.snapTo(at.x, at.y, at.z);
                    player.setDeltaMovement(Vec3.ZERO);
                    return player;
                }
                case "item" -> {
                    ItemEntity item = new ItemEntity(level, at.x, at.y, at.z, new ItemStack(Items.COBBLESTONE));
                    item.setDeltaMovement(Vec3.ZERO);
                    level.addFreshEntity(item);
                    return item;
                }
                default -> {
                    Pig pig = EntityTypes.PIG.create(level, EntitySpawnReason.COMMAND);
                    pig.snapTo(at.x, at.y, at.z);
                    pig.setBaby(false);
                    pig.removeFreeWill();
                    pig.setDeltaMovement(Vec3.ZERO);
                    level.addFreshEntity(pig);
                    return pig;
                }
            }
        }

        void record() {
            if (!recording || tick > ENTITY_TICKS) {
                return;
            }
            for (Run run : runs) {
                Entity e = run.entity;
                if (e instanceof ServerPlayer player) {
                    player.doTick();
                }
                JsonObject line = obj("tick", tick, "fluid", run.fluid, "kind", run.kind,
                      "pos", arr(rnd(e.getX() - origin.getX()), rnd(e.getY() - origin.getY()), rnd(e.getZ() - origin.getZ())),
                      "motion", arr(rnd(e.getDeltaMovement().x), rnd(e.getDeltaMovement().y), rnd(e.getDeltaMovement().z)),
                      "in_water", e.isInWater(), "in_lava", e.isInLava(), "under_water", e.isUnderWater(),
                      "water_height", rnd(e.getFluidHeight(FluidTags.WATER)), "lava_height", rnd(e.getFluidHeight(FluidTags.LAVA)),
                      "fire", e.getRemainingFireTicks(), "on_fire", e.isOnFire(), "alive", e.isAlive());
                if (e instanceof LivingEntity living) {
                    line.add("health", el(rnd(living.getHealth())));
                    line.add("air", el(living.getAirSupply()));
                }
                lines.add(line.toString());
            }
            tick++;
        }

        void write() {
            recording = false;
            FluidoNoBlocoTraceTest.write("fluido_no_bloco_entity", lines);
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------------ bucket

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the buckets of a fake player on empty and full fluid logged blocks, standing and sneaking.")
    public static void fluidoNoBlocoBucket(final DynamicTest test) {
        template(test, 40, 8, 40);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> bucket(helper))
              .thenSucceed());
    }

    private static String stackName(ItemStack stack) {
        return stack.isEmpty() ? "empty" : BuiltInRegistries.ITEM.getKey(stack.getItem()) + "x" + stack.getCount();
    }

    private static void bucket(MekGameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        BlockPos origin = helper.absolutePos(BlockPos.ZERO);
        List<String> lines = new ArrayList<>();
        // Os blocos do cenário: o cabo (sem tela), a fábrica (com tela) e o Heat Generator-like que só tem `empty` e `lava` fica de fora (é do Generators).
        String[] blocks = {"mekanism:basic_universal_cable", "mekanism:basic_smelting_factory", "mekanism:basic_energy_cube"};
        int index = 0;
        java.util.Map<String, ServerPlayer> players = new java.util.HashMap<>();
        for (String mode : new String[]{"stand", "sneak", "creative"}) {
            ServerPlayer player = helper.makeTickingMockServerPlayerInLevel(mode.equals("creative") ? GameType.CREATIVE : GameType.SURVIVAL);
            level.getServer().getConnection().getConnections().remove(player.connection.getConnection());
            player.setShiftKeyDown(mode.equals("sneak"));
            players.put(mode, player);
        }
        for (String id : blocks) {
            for (String start : new String[]{"empty", "water", "lava"}) {
                for (String item : new String[]{"water_bucket", "lava_bucket", "bucket"}) {
                    for (String mode : new String[]{"stand", "sneak", "creative"}) {
                        BlockPos pos = origin.offset(2 + 3 * (index % 12), 1, 2 + 3 * (index / 12));
                        index++;
                        level.setBlock(pos, block(id).defaultBlockState(), 3);
                        if (!start.equals("empty")) {
                            place(level, pos, id, start.equals("water"));
                        }
                        ServerPlayer player = players.get(mode);
                        player.getInventory().clearContent();
                        // Mira de cima o topo do bloco, como o cliente: primeiro `useItemOn` (o bloco e a tela), e se passar, `useItem` (o balde).
                        Vec3 center = Vec3.atCenterOf(pos);
                        Vec3 eye = center.relative(Direction.UP, 2.5);
                        player.snapTo(eye.x, eye.y - player.getEyeHeight(), eye.z);
                        player.setDeltaMovement(Vec3.ZERO);
                        player.lookAt(EntityAnchorArgument.Anchor.EYES, center.relative(Direction.UP, 0.4));
                        ItemStack held = new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.parse("minecraft:" + item)));
                        player.setItemInHand(InteractionHand.MAIN_HAND, held);
                        BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(pos).add(0, 0.5, 0), Direction.UP, pos, false);
                        InteractionResult onBlock = player.gameMode.useItemOn(player, level, held, InteractionHand.MAIN_HAND, hit);
                        boolean used = false;
                        if (!onBlock.consumesAction()) {
                            player.gameMode.useItem(player, level, player.getItemInHand(InteractionHand.MAIN_HAND), InteractionHand.MAIN_HAND);
                            used = true;
                        }
                        BlockState after = level.getBlockState(pos);
                        BlockState above = level.getBlockState(pos.above());
                        lines.add(obj("block", id, "start", start, "item", item, "mode", mode, "on_block", onBlock.getClass().getSimpleName() + ":" + onBlock,
                              "used", used, "state", after.getValue(BlockStateHelper.FLUID_LOGGED).getSerializedName(), "above", describe(above),
                              "above_fluid", fluidOf(above), "hand", stackName(player.getItemInHand(InteractionHand.MAIN_HAND)),
                              "water_buckets", player.getInventory().countItem(Items.WATER_BUCKET), "lava_buckets", player.getInventory().countItem(Items.LAVA_BUCKET),
                              "buckets", player.getInventory().countItem(Items.BUCKET)).toString());
                    }
                }
            }
        }
        // As consultas sozinhas.
        for (String id : blocks) {
            BlockState state = block(id).defaultBlockState();
            IStateFluidLoggable loggable = (IStateFluidLoggable) state.getBlock();
            lines.add(obj("block", id, "can_water", loggable.canPlaceLiquid(null, level, origin, state, Fluids.WATER), "can_lava", loggable.canPlaceLiquid(null, level, origin, state, Fluids.LAVA),
                  "valid_water", loggable.isValidFluid(Fluids.WATER), "light_lava", loggable.getFluidLightLevel(state.setValue(BlockStateHelper.FLUID_LOGGED, FluidLogType.LAVA)),
                  "light_water", loggable.getFluidLightLevel(state.setValue(BlockStateHelper.FLUID_LOGGED, FluidLogType.WATER))).toString());
        }
        write("fluido_no_bloco_bucket", lines);
    }
}
