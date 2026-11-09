package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonPrimitive;
import com.mojang.serialization.Codec;
import java.io.IOException;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collection;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.TreeMap;
import java.util.UUID;
import mekanism.common.registration.impl.FluidDeferredRegister.MekanismFluidType;
import mekanism.common.registration.impl.FluidRegistryObject;
import mekanism.common.registries.MekanismFluids;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.generators.common.registries.GeneratorsFluids;
import net.minecraft.SharedConstants;
import net.minecraft.commands.arguments.EntityAnchorArgument;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponentType;
import net.minecraft.core.component.TypedDataComponent;
import net.minecraft.core.dispenser.DispenseItemBehavior;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.tags.TagKey;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.animal.pig.Pig;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.BucketItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.Consumable;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.DispenserBlock;
import net.minecraft.world.level.block.DoublePlantBlock;
import net.minecraft.world.level.block.LiquidBlock;
import net.minecraft.world.level.block.entity.DispenserBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateHolder;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.world.level.block.state.properties.DoubleBlockHalf;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.levelgen.structure.BoundingBox;
import net.minecraft.world.level.material.FlowingFluid;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.level.pathfinder.PathType;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.ticks.LevelChunkTicks;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.common.SoundActions;
import net.neoforged.neoforge.event.PlayLevelSoundEvent;
import net.neoforged.neoforge.event.VanillaGameEvent;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.fluids.FluidStack;
import net.neoforged.neoforge.fluids.FluidType;
import net.neoforged.neoforge.transfer.access.ItemAccess;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava os 22 fluidos do original (`MekanismFluids` e `GeneratorsFluids`) e o que eles fazem no mundo.
///
/// - `fluids.jsonl`: uma linha por fluido, na ordem de registro: `type` (o `MekanismFluidType`), `fluid` (o still e o flowing), `block` (o `LiquidBlock`) e
///   `bucket` (o `BucketItem`, os componentes, o dispensador e a capability de fluido).
/// - `fluids_bucket.jsonl`: o balde cheio e o vazio, com um jogador falso em sobrevivência e em criativo, o dispensador e o balde de pasta nutritiva bebido.
/// - `fluids_flow_<cenário>.jsonl`: o mundo simulado, tick a tick, com `setBlockAndUpdate`. Cada linha é um tick em que algo mudou no volume (uma célula, a
///   contagem de fluid ticks agendados no volume ou um item solto). `cells` leva `[x, y, z, id, level]` das células cujo bloco mudou desde a linha anterior; a
///   primeira linha (`full`) leva todas as células que não são ar. A última linha é o tick em que o cenário parou (40 ticks sem mudança) ou o último tick.
/// - `fluids_entity.jsonl`: jogador, item e porco soltos em colunas de fluido (e de controle), tick a tick, mais um canal de fluxo (`level` 3).
///
/// Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace")
public class FluidsTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final String BATCH = "trace_fluids";
    private static final String BATCH_FLOW = "trace_fluids_flow";
    private static final int FLOW_MAX_TICKS = 200;
    private static final int FLOW_STILL_TICKS = 40;

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
        } else if (value instanceof String string) {
            return new JsonPrimitive(string);
        } else if (value instanceof Enum<?> constant) {
            return new JsonPrimitive(constant.name());
        } else if (value instanceof Optional<?> optional) {
            return el(optional.orElse(null));
        } else if (value instanceof Collection<?> collection) {
            JsonArray array = new JsonArray();
            collection.forEach(each -> array.add(el(each)));
            return array;
        } else if (value instanceof Object[] objects) {
            JsonArray array = new JsonArray();
            for (Object each : objects) {
                array.add(el(each));
            }
            return array;
        } else if (value instanceof int[] ints) {
            JsonArray array = new JsonArray();
            for (int each : ints) {
                array.add(each);
            }
            return array;
        } else if (value instanceof Vec3 vec) {
            return arr(vec.x, vec.y, vec.z);
        } else {
            return new JsonPrimitive(String.valueOf(value));
        }
    }

    /// As coordenadas absolutas mudam de uma execução para outra (a estrutura do teste nasce em outro lugar), e a diferença de arredondamento é de 1e-10: o rastro guarda 5 casas.
    private static double rnd(double value) {
        return Math.round(value * 1.0E5) / 1.0E5;
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

    private static void write(String name, List<String> lines) {
        try {
            Path file = Path.of("trace", name + ".jsonl");
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    @SuppressWarnings("unchecked")
    private static <T> T call(Object target, Class<?> owner, String name, Class<?>[] types, Object... args) {
        try {
            Method method = owner.getDeclaredMethod(name, types);
            method.setAccessible(true);
            return (T) method.invoke(target, args);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(owner.getSimpleName() + "." + name, e);
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------------ ids

    private static String id(Block block) {
        return BuiltInRegistries.BLOCK.getKey(block).toString();
    }

    private static String id(Item item) {
        return BuiltInRegistries.ITEM.getKey(item).toString();
    }

    private static String id(Fluid fluid) {
        return BuiltInRegistries.FLUID.getKey(fluid).toString();
    }

    private static Integer levelOf(BlockState state) {
        return state.hasProperty(BlockStateProperties.LEVEL) ? state.getValue(BlockStateProperties.LEVEL) : null;
    }

    private static JsonObject props(StateHolder<?, ?> state) {
        JsonObject out = new JsonObject();
        state.getValues().forEach(value -> {
            Object v = value.value();
            out.add(value.property().getName(), v instanceof Boolean || v instanceof Number ? el(v) : new JsonPrimitive(String.valueOf(v)));
        });
        return out;
    }

    private static JsonElement stack(ItemStack stack) {
        return stack.isEmpty() ? obj("item", "minecraft:air", "count", 0) : obj("item", id(stack.getItem()), "count", stack.getCount());
    }

    private static String soundId(net.minecraft.sounds.SoundEvent sound) {
        return sound == null ? null : BuiltInRegistries.SOUND_EVENT.getKey(sound).toString();
    }

    private static List<FluidRegistryObject<MekanismFluidType, net.neoforged.neoforge.fluids.BaseFlowingFluid.Source, net.neoforged.neoforge.fluids.BaseFlowingFluid.Flowing, LiquidBlock, ? extends BucketItem>> all() {
        List<FluidRegistryObject<MekanismFluidType, net.neoforged.neoforge.fluids.BaseFlowingFluid.Source, net.neoforged.neoforge.fluids.BaseFlowingFluid.Flowing, LiquidBlock, ? extends BucketItem>> all = new ArrayList<>();
        all.addAll(MekanismFluids.FLUIDS.getEntries());
        all.addAll(GeneratorsFluids.FLUIDS.getEntries());
        return all;
    }

    private static Block fluidBlock(String path) {
        Block block = BuiltInRegistries.BLOCK.getValue(Identifier.parse(path));
        if (block == Blocks.AIR) {
            throw new IllegalStateException("no block " + path);
        }
        return block;
    }

    private static Item item(String path) {
        Item item = BuiltInRegistries.ITEM.getValue(Identifier.parse(path));
        if (item == Items.AIR) {
            throw new IllegalStateException("no item " + path);
        }
        return item;
    }

    private static void template(DynamicTest test, int sx, int sy, int sz) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(sx, sy, sz).fill(0, 0, 0, sx - 1, 0, sz - 1, Blocks.STONE.defaultBlockState()));
    }

    /// Os fluid ticks agendados dentro do volume (e não no nível inteiro, onde os outros testes em paralelo também agendam).
    private static int pendingFluidTicks(ServerLevel level, BlockPos origin, int sx, int sy, int sz) {
        BoundingBox box = new BoundingBox(origin.getX(), origin.getY(), origin.getZ(), origin.getX() + sx - 1, origin.getY() + sy - 1, origin.getZ() + sz - 1);
        int count = 0;
        for (int cx = origin.getX() >> 4; cx <= (origin.getX() + sx - 1) >> 4; cx++) {
            for (int cz = origin.getZ() >> 4; cz <= (origin.getZ() + sz - 1) >> 4; cz++) {
                LevelChunk chunk = level.getChunk(cx, cz);
                if (chunk.getFluidTicks() instanceof LevelChunkTicks<Fluid> ticks) {
                    count += (int) ticks.getAll().filter(tick -> box.isInside(tick.pos())).count();
                }
            }
        }
        return count;
    }

    // ------------------------------------------------------------------------------------------------------------------------------ fluids.jsonl

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the FluidType, the fluids, the liquid block and the bucket of the 22 fluids.")
    public static void fluidsRegistry(final DynamicTest test) {
        template(test, 24, 4, 8);
        Map<String, Integer> light = new java.util.LinkedHashMap<>();
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> placeLight(helper))
              .thenExecuteAfter(10, () -> readLight(helper, light))
              .thenExecute(() -> registry(helper, light))
              .thenSucceed());
    }

    /// Cada fonte numa célula de 1 por 1 fechada de pedra (a fonte não escorre e a luz dos vizinhos não entra), para medir a luz do bloco no mundo depois de o motor de luz rodar.
    private static BlockPos lightCell(int index) {
        return new BlockPos(1 + 2 * (index % 11), 1, 1 + 2 * (index / 11));
    }

    private static void placeLight(MekGameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        for (int x = 0; x <= 22; x++) {
            for (int z = 0; z <= 6; z++) {
                if (x % 2 == 0 || z % 2 == 0) {
                    level.setBlock(helper.absolutePos(new BlockPos(x, 1, z)), Blocks.STONE.defaultBlockState(), 2);
                }
                //Tampa de pedra em cima de tudo: a luz de uma célula não chega a outra.
                level.setBlock(helper.absolutePos(new BlockPos(x, 2, z)), Blocks.STONE.defaultBlockState(), 2);
            }
        }
        int index = 0;
        for (var f : all()) {
            level.setBlock(helper.absolutePos(lightCell(index++)), f.getBlock().defaultBlockState(), 2);
        }
        //Os controles, que têm luz: o motor de luz está funcionando.
        level.setBlock(helper.absolutePos(lightCell(index++)), Blocks.LAVA.defaultBlockState(), 2);
        level.setBlock(helper.absolutePos(lightCell(index)), Blocks.GLOWSTONE.defaultBlockState(), 2);
    }

    private static void readLight(MekGameTestHelper helper, Map<String, Integer> light) {
        ServerLevel level = helper.getLevel();
        int index = 0;
        for (var f : all()) {
            BlockPos pos = helper.absolutePos(lightCell(index++));
            light.put(f.getId().toString(), level.getBrightness(net.minecraft.world.level.LightLayer.BLOCK, pos));
        }
        light.put("control:minecraft:lava", level.getBrightness(net.minecraft.world.level.LightLayer.BLOCK, helper.absolutePos(lightCell(index++))));
        light.put("control:minecraft:glowstone", level.getBrightness(net.minecraft.world.level.LightLayer.BLOCK, helper.absolutePos(lightCell(index))));
    }

    private static void registry(MekGameTestHelper helper, Map<String, Integer> light) {
        ServerLevel level = helper.getLevel();
        ServerLevel nether = level.getServer().getLevel(Level.NETHER);
        BlockPos pos = helper.absolutePos(new BlockPos(1, 1, 1));
        Pig pig = EntityTypes.PIG.create(level, EntitySpawnReason.COMMAND);
        net.minecraft.world.entity.vehicle.boat.AbstractBoat boat = (net.minecraft.world.entity.vehicle.boat.AbstractBoat) EntityTypes.OAK_BOAT.create(level, EntitySpawnReason.COMMAND);
        List<String> lines = new ArrayList<>();
        for (var f : all()) {
            Identifier fluidId = f.getId();
            lines.add(obj("mod", fluidId.getNamespace(), "name", fluidId.getPath(), "type", typeJson(level, nether, pos, f.getFluidType(), f.get(), pig, boat),
                  "fluid", fluidJson(level, pos, f.get(), f.getFlowingFluid().get()), "block", blockJson(level, pos, f.getBlock(), light.get(fluidId.toString()), new int[]{light.get("control:minecraft:lava"), light.get("control:minecraft:glowstone")}),
                  "bucket", bucketJson(level, f.getBucket().get())).toString());
        }
        write("fluids", lines);
    }

    private static JsonObject typeJson(ServerLevel level, ServerLevel nether, BlockPos pos, MekanismFluidType type, Fluid still, Pig pig,
          net.minecraft.world.entity.vehicle.boat.AbstractBoat boat) {
        FluidState state = still.defaultFluidState();
        FluidStack stack = new FluidStack(still, FluidType.BUCKET_VOLUME);
        JsonObject out = obj("temperature", type.getTemperature(), "density", type.getDensity(), "viscosity", type.getViscosity(), "lightLevel", type.getLightLevel(),
              "rarity", type.getRarity(), "descriptionId", type.getDescriptionId(), "motionScale", type.motionScale(pig), "canPushEntity", type.canPushEntity(pig),
              "canSwim", type.canSwim(pig), "canDrown", type.canDrownIn(pig), "fallDistanceModifier", type.getFallDistanceModifier(pig),
              "fallDistanceModifier_bits", Float.floatToRawIntBits(type.getFallDistanceModifier(pig)), "canExtinguish", type.canExtinguish(pig),
              "canExtinguish_state", type.canExtinguish(state, level, pos), "canConvertToSource_state", type.canConvertToSource(state, level, pos),
              "canConvertToSource_stack", type.canConvertToSource(stack), "supportsBoating", boat == null ? null : type.supportsBoating(boat),
              "supportsBoating_state", boat == null ? null : type.supportsBoating(state, boat), "canHydrate", type.canHydrate(pig),
              "canHydrate_stack", type.canHydrate(stack), "isLighterThanAir", type.isLighterThanAir(), "isWaterLike", type.getIsWaterLike(),
              "pathType", type.getBlockPathType(state, level, pos, null, false), "pathType_fluidlog", type.getBlockPathType(state, level, pos, null, true),
              "adjacentPathType", type.getAdjacentBlockPathType(state, level, pos, null, PathType.WALKABLE),
              "isVanilla", type.isVanilla(), "isAir", type.isAir(), "toString", type.toString());
        out.add("sound_bucket_fill", el(soundId(type.getSound(SoundActions.BUCKET_FILL))));
        out.add("sound_bucket_empty", el(soundId(type.getSound(SoundActions.BUCKET_EMPTY))));
        out.add("sound_fluid_vaporize", el(soundId(type.getSound(SoundActions.FLUID_VAPORIZE))));
        out.add("sound_bucket_empty_at", el(soundId(type.getSound(null, level, pos, SoundActions.BUCKET_EMPTY))));
        out.add("vaporized_overworld", el(type.isVaporizedOnPlacement(level, pos, stack)));
        out.add("vaporized_nether", el(nether == null ? null : type.isVaporizedOnPlacement(nether, pos, stack)));
        FluidType.DripstoneDripInfo drip = type.getDripInfo();
        out.add("dripInfo", drip == null ? JsonNull.INSTANCE : obj("chance", drip.chance(), "particle", drip.dripParticle() == null ? null : String.valueOf(drip.dripParticle()),
              "cauldron", id(drip.filledCauldron())));
        out.add("stack_lightLevel", el(type.getLightLevel(stack)));
        out.add("stack_density", el(type.getDensity(stack)));
        out.add("stack_temperature", el(type.getTemperature(stack)));
        out.add("stack_viscosity", el(type.getViscosity(stack)));
        out.add("stack_rarity", el(type.getRarity(stack)));
        out.add("state_lightLevel", el(type.getLightLevel(state, level, pos)));
        out.add("state_density", el(type.getDensity(state, level, pos)));
        out.add("state_temperature", el(type.getTemperature(state, level, pos)));
        out.add("state_viscosity", el(type.getViscosity(state, level, pos)));
        out.add("bucket_of_stack", stack(type.getBucket(stack)));
        out.add("block_for_state", el(id(type.getBlockForFluidState(level, pos, state).getBlock())));
        out.add("stillTexture", el(type.stillTexture));
        out.add("flowingTexture", el(type.flowingTexture));
        out.add("overlayTexture", el(type.overlayTexture));
        out.add("renderOverlayTexture", el(type.renderOverlayTexture));
        out.add("color", el(type.color));
        out.add("color_hex", el(String.format("0x%08X", type.color)));
        return out;
    }

    private static JsonObject fluidJson(ServerLevel level, BlockPos pos, Fluid stillFluid, Fluid flowingFluid) {
        FlowingFluid still = (FlowingFluid) stillFluid;
        FlowingFluid flowing = (FlowingFluid) flowingFluid;
        return obj("still", fluidClass(level, pos, still), "flowing", fluidClass(level, pos, flowing), "isSame_still_flowing", still.isSame(flowing),
              "isSame_flowing_still", flowing.isSame(still), "isSame_still_still", still.isSame(still), "isSame_still_water", still.isSame(Fluids.WATER),
              "isSame_still_empty", still.isSame(Fluids.EMPTY), "isSame_water_still", Fluids.WATER.isSame(still), "still_is_source", still.defaultFluidState().isSource(),
              "flowing_is_source", flowing.defaultFluidState().isSource(), "still_getFlowing", id(still.getFlowing()), "flowing_getSource", id(flowing.getSource()),
              "getFlowing_8_false", props(still.getFlowing(8, false)), "getSource_true", props(still.getSource(true)));
    }

    private static JsonObject fluidClass(ServerLevel level, BlockPos pos, FlowingFluid fluid) {
        FluidState state = fluid.defaultFluidState();
        float explosion = call(fluid, Fluid.class, "getExplosionResistance", new Class<?>[0]);
        int slope = call(fluid, FlowingFluid.class, "getSlopeFindDistance", new Class<?>[]{LevelReader.class}, level);
        int dropOff = call(fluid, FlowingFluid.class, "getDropOff", new Class<?>[]{LevelReader.class}, level);
        boolean convert = call(fluid, FlowingFluid.class, "canConvertToSource", new Class<?>[]{ServerLevel.class}, level);
        boolean randomly = call(fluid, Fluid.class, "isRandomlyTicking", new Class<?>[0]);
        return obj("class", fluid.getClass().getName(), "id", id(fluid), "tickDelay", fluid.getTickDelay(level), "slopeFindDistance", slope, "dropOff", dropOff,
              "explosionResistance", explosion, "explosionResistance_bits", Float.floatToRawIntBits(explosion), "canConvertToSource", convert,
              "canConvertToSource_state", fluid.canConvertToSource(state, level, pos), "bucket", id(fluid.getBucket()),
              "pickupSound", fluid.getPickupSound().map(FluidsTraceTest::soundId).orElse(null), "defaultFluidState", props(state), "isSource", state.isSource(),
              "amount", state.getAmount(), "isEmpty", state.isEmpty(), "ownHeight", fluid.getOwnHeight(state), "isRandomlyTicking", randomly,
              "fluidType_same", fluid.getFluidType() == fluid.getFluidType(), "createLegacyBlock", obj("block", id(state.createLegacyBlock().getBlock()),
                    "level", levelOf(state.createLegacyBlock())),
              "tags", fluid.builtInRegistryHolder().tags().map(TagKey::location).map(Identifier::toString).sorted().toList());
    }

    private static JsonObject blockJson(ServerLevel level, BlockPos pos, LiquidBlock block, int worldLight, int[] controls) {
        BlockState def = block.defaultBlockState();
        JsonArray levels = new JsonArray();
        for (int l = 0; l < 16; l++) {
            BlockState state = def.setValue(LiquidBlock.LEVEL, l);
            FluidState fluid = state.getFluidState();
            levels.add(obj("level", l, "lightEmission", state.getLightEmission(), "lightEmission_world", state.getLightEmission(level, pos), "fluid", id(fluid.getType()),
                  "isSource", fluid.isSource(), "amount", fluid.getAmount(), "falling", fluid.isEmpty() ? null : fluid.getValue(FlowingFluid.FALLING)));
        }
        net.minecraft.world.level.block.SoundType sound = def.getSoundType();
        return obj("id", id(block), "class", block.getClass().getName(), "defaultState", props(def), "properties", block.getStateDefinition().getProperties().stream()
                    .map(net.minecraft.world.level.block.state.properties.Property::getName).toList(),
              "states", block.getStateDefinition().getPossibleStates().size(), "lightEmission_level0", def.getLightEmission(), "block_light_in_world", worldLight, "block_light_controls", obj("lava", controls[0], "glowstone", controls[1]), "levels", levels,
              "destroySpeed", def.getDestroySpeed(level, pos), "destroySpeed_bits", Float.floatToRawIntBits(def.getDestroySpeed(level, pos)),
              "explosionResistance", block.getExplosionResistance(), "explosionResistance_bits", Float.floatToRawIntBits(block.getExplosionResistance()),
              "isRandomlyTicking", def.isRandomlyTicking(), "canOcclude", def.canOcclude(), "pushReaction", def.getPistonPushReaction(),
              "replaceable", def.canBeReplaced(), "replaceable_by_water", def.canBeReplaced(Fluids.WATER), "liquid", def.liquid(), "isAir", def.isAir(),
              "collision_empty", def.getCollisionShape(level, pos).isEmpty(), "shape_empty", def.getShape(level, pos).isEmpty(), "renderShape", def.getRenderShape(),
              "mapColor_id", def.getMapColor(level, pos).id, "mapColor_col", def.getMapColor(level, pos).col, "lootTable", block.getLootTable().map(Object::toString).orElse(null),
              "soundType", obj("volume", sound.getVolume(), "pitch", sound.getPitch(), "break", soundId(sound.getBreakSound()), "step", soundId(sound.getStepSound()),
                    "place", soundId(sound.getPlaceSound()), "hit", soundId(sound.getHitSound()), "fall", soundId(sound.getFallSound())),
              "blockItem", id(block.asItem()), "tags", def.tags().map(TagKey::location).map(Identifier::toString).sorted().toList(),
              "pickupSound", block.getPickupSound().map(FluidsTraceTest::soundId).orElse(null));
    }

    private static JsonObject bucketJson(ServerLevel level, BucketItem bucket) {
        ItemStack stack = bucket.getDefaultInstance();
        RegistryOps<JsonElement> ops = level.registryAccess().createSerializationContext(com.mojang.serialization.JsonOps.INSTANCE);
        JsonObject components = new JsonObject();
        //A ordem do `DataComponentMap` muda a cada execução do JVM: as chaves saem em ordem alfabética.
        Map<String, JsonElement> sorted = new TreeMap<>();
        for (TypedDataComponent<?> component : stack.getComponents()) {
            sorted.put(BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(component.type()).toString(), encode(ops, component));
        }
        sorted.forEach(components::add);
        Object handler = stack.getCapability(Capabilities.Fluid.ITEM, ItemAccess.forStack(stack));
        DispenseItemBehavior dispense = DispenserBlock.DISPENSER_REGISTRY.get(bucket);
        return obj("id", id(bucket), "class", bucket.getClass().getName(), "content", id(bucket.content), "maxStackSize", bucket.getDefaultMaxStackSize(),
              "stackMaxStackSize", stack.getMaxStackSize(), "rarity", stack.getRarity(), "craftRemainder", stack.getCraftingRemainder() == null ? null : id(stack.getCraftingRemainder().item().value()),
              "defaultStack", obj("item", id(stack.getItem()), "count", stack.getCount(), "components", components), "dispenser", dispense == null ? null : dispense.getClass().getName(),
              "fluidCapability", handler == null ? null : handler.getClass().getName(), "tags", stack.typeHolder().tags().map(TagKey::location).map(Identifier::toString).sorted().toList());
    }

    private static <T> JsonElement encode(RegistryOps<JsonElement> ops, TypedDataComponent<T> component) {
        DataComponentType<T> type = component.type();
        Codec<T> codec = type.codec();
        if (codec == null) {
            return new JsonPrimitive(String.valueOf(component.value()));
        }
        return codec.encodeStart(ops, component.value()).result().orElseGet(() -> new JsonPrimitive("unencodable " + component.value()));
    }

    // ------------------------------------------------------------------------------------------------------------------------------ captura de som e eventos

    /// Os sons e os eventos de jogo que acontecem enquanto uma ação do cenário roda (tudo no mesmo tick, no thread do servidor).
    private static final class Capture {
        private final List<JsonElement> sounds = new ArrayList<>();
        private final List<JsonElement> events = new ArrayList<>();
        private boolean on;
        private BlockPos origin = BlockPos.ZERO;

        void begin(BlockPos origin) {
            this.origin = origin;
            sounds.clear();
            events.clear();
            on = true;
        }

        void end(JsonObject into) {
            on = false;
            into.add("sounds", el(new ArrayList<>(sounds)));
            into.add("game_events", el(new ArrayList<>(events)));
        }

        void sound(PlayLevelSoundEvent.AtPosition event) {
            if (on) {
                Vec3 p = event.getPosition();
                sounds.add(obj("sound", event.getSound().value().location().toString(), "source", event.getSource(), "pos", arr(rnd(p.x - origin.getX()), rnd(p.y - origin.getY()), rnd(p.z - origin.getZ())),
                      "volume", event.getNewVolume(), "pitch", event.getNewPitch()));
            }
        }

        void gameEvent(VanillaGameEvent event) {
            if (on) {
                Vec3 p = event.getEventPosition();
                events.add(obj("event", event.getVanillaEvent().getRegisteredName(), "cause", event.getCause() == null ? null : event.getCause().getType().toShortString(),
                      "pos", arr(rnd(p.x - origin.getX()), rnd(p.y - origin.getY()), rnd(p.z - origin.getZ()))));
            }
        }
    }

    private static JsonElement result(InteractionResult result) {
        if (result instanceof InteractionResult.Success success) {
            return obj("kind", "Success", "swing", success.swingSource(), "consumesAction", result.consumesAction(), "heldItemTransformedTo",
                  success.heldItemTransformedTo() == null ? null : stack(success.heldItemTransformedTo()));
        }
        return obj("kind", result.getClass().getSimpleName(), "consumesAction", result.consumesAction());
    }

    // ------------------------------------------------------------------------------------------------------------------------------ fluids_bucket.jsonl

    private static final String[] BUCKET_FLUIDS = {"mekanism:brine", "mekanism:hydrogen", "mekanism:superheated_sodium", "mekanism:nutritional_paste"};
    /// Palco do balde: x e z de 0 a 6, y de 0 (piso de pedra) a 5.
    private static final int STAGE = 7;
    private static final int STAGE_HEIGHT = 6;
    private static final int PASTE_TICKS = 60;

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records the bucket of four fluids used by a fake player, the dispenser and the nutritional paste bucket being drunk.")
    public static void fluidsBucket(final DynamicTest test) {
        template(test, 16, STAGE_HEIGHT + 2, 16);
        Bucket scene = new Bucket();
        test.eventListeners().forge().addListener((final PlayLevelSoundEvent.AtPosition event) -> scene.capture.sound(event));
        test.eventListeners().forge().addListener((final VanillaGameEvent event) -> scene.capture.gameEvent(event));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.start(helper))
              .thenExecute(scene::synchronous)
              .thenExecuteFor(12 * (PASTE_TICKS + 4) + 10, scene::pasteStep)
              .thenExecute(scene::write)
              .thenSucceed());
    }

    private static final class Bucket {
        final Capture capture = new Capture();
        final List<String> lines = new ArrayList<>();
        MekGameTestHelper helper;
        ServerLevel level;
        BlockPos origin;
        ServerPlayer survival;
        ServerPlayer creative;
        final List<Runnable> pasteCases = new ArrayList<>();
        int pasteIndex;
        int pasteTicks = -1;
        ServerPlayer pastePlayer;
        String pasteName;

        void start(MekGameTestHelper helper) {
            this.helper = helper;
            this.level = helper.getLevel();
            this.origin = helper.absolutePos(BlockPos.ZERO);
            survival = helper.makeTickingMockServerPlayerInLevel(GameType.SURVIVAL);
            creative = helper.makeTickingMockServerPlayerInLevel(GameType.CREATIVE);
            park(survival);
            park(creative);
        }

        BlockPos abs(int x, int y, int z) {
            return origin.offset(x, y, z);
        }

        void park(ServerPlayer player) {
            player.snapTo(origin.getX() + 12.5, origin.getY() + 1, origin.getZ() + 12.5);
            player.setDeltaMovement(Vec3.ZERO);
        }

        /// Piso de pedra de 7 por 7, ar em cima, sem itens soltos nem fluid ticks.
        void reset() {
            for (int x = 0; x < STAGE; x++) {
                for (int z = 0; z < STAGE; z++) {
                    for (int y = 0; y < STAGE_HEIGHT; y++) {
                        level.setBlock(abs(x, y, z), y == 0 ? Blocks.STONE.defaultBlockState() : Blocks.AIR.defaultBlockState(), 2);
                    }
                }
            }
            AABB box = new AABB(origin.getX(), origin.getY(), origin.getZ(), origin.getX() + STAGE, origin.getY() + STAGE_HEIGHT, origin.getZ() + STAGE);
            level.getEntitiesOfClass(ItemEntity.class, box.inflate(1)).forEach(Entity::discard);
            level.getFluidTicks().clearArea(new BoundingBox(origin.getX(), origin.getY(), origin.getZ(), origin.getX() + STAGE, origin.getY() + STAGE_HEIGHT, origin.getZ() + STAGE));
        }

        BlockState[][][] snapshot() {
            BlockState[][][] cells = new BlockState[STAGE][STAGE_HEIGHT][STAGE];
            for (int x = 0; x < STAGE; x++) {
                for (int y = 0; y < STAGE_HEIGHT; y++) {
                    for (int z = 0; z < STAGE; z++) {
                        cells[x][y][z] = level.getBlockState(abs(x, y, z));
                    }
                }
            }
            return cells;
        }

        JsonArray diff(BlockState[][][] before, BlockState[][][] after) {
            JsonArray out = new JsonArray();
            for (int x = 0; x < STAGE; x++) {
                for (int y = 0; y < STAGE_HEIGHT; y++) {
                    for (int z = 0; z < STAGE; z++) {
                        if (before[x][y][z] != after[x][y][z]) {
                            out.add(arr(x, y, z, id(before[x][y][z].getBlock()), levelOf(before[x][y][z]), id(after[x][y][z].getBlock()), levelOf(after[x][y][z])));
                        }
                    }
                }
            }
            return out;
        }

        JsonArray items() {
            JsonArray out = new JsonArray();
            AABB box = new AABB(origin.getX(), origin.getY(), origin.getZ(), origin.getX() + STAGE, origin.getY() + STAGE_HEIGHT, origin.getZ() + STAGE).inflate(2);
            List<ItemEntity> found = new ArrayList<>(level.getEntitiesOfClass(ItemEntity.class, box));
            found.sort((a, b) -> Double.compare(a.getX(), b.getX()));
            for (ItemEntity entity : found) {
                // A grama solta sementes de trigo ao acaso (12,5 %): a semente não entra, para duas rodadas darem os mesmos bytes.
                if (entity.getItem().is(Items.WHEAT_SEEDS)) {
                    continue;
                }
                //A velocidade inicial do item solto sai do sorteio do nível: não entra.
                out.add(obj("stack", stack(entity.getItem()), "pos", arr(rnd(entity.getX() - origin.getX()), rnd(entity.getY() - origin.getY()), rnd(entity.getZ() - origin.getZ()))));
            }
            return out;
        }

        void aim(ServerPlayer player, BlockPos target, Direction face, double distance) {
            Vec3 center = Vec3.atCenterOf(target);
            Vec3 eye = center.relative(face, 0.5 + distance);
            player.snapTo(eye.x, eye.y - player.getEyeHeight(), eye.z);
            player.setDeltaMovement(Vec3.ZERO);
            player.lookAt(EntityAnchorArgument.Anchor.EYES, center.relative(face, 0.4));
        }

        void aimSky(ServerPlayer player) {
            player.snapTo(origin.getX() + 3.5, origin.getY() + 1, origin.getZ() + 3.5);
            player.setDeltaMovement(Vec3.ZERO);
            player.lookAt(EntityAnchorArgument.Anchor.EYES, new Vec3(origin.getX() + 3.5, origin.getY() + 40, origin.getZ() + 3.5));
        }

        /// Um caso: monta o palco, deixa o jogador mirar, usa o item como `ServerPlayerGameMode.useItem` e grava o que mudou.
        void useCase(String fluid, ServerPlayer player, String mode, String name, ItemStack held, Runnable build, Runnable aim) {
            reset();
            build.run();
            aim.run();
            player.setItemInHand(InteractionHand.MAIN_HAND, held);
            BlockState[][][] before = snapshot();
            int used = player.getStats().getValue(net.minecraft.stats.Stats.ITEM_USED.get(held.getItem()));
            capture.begin(origin);
            InteractionResult result = player.gameMode.useItem(player, level, held, InteractionHand.MAIN_HAND);
            JsonObject line = obj("kind", "use", "fluid", fluid, "mode", mode, "case", name, "held_item", id(held.getItem()), "result", result(result));
            capture.end(line);
            line.add("hand_after", stack(player.getItemInHand(InteractionHand.MAIN_HAND)));
            line.add("cells", diff(before, snapshot()));
            line.add("items", items());
            line.add("fluid_ticks", el(pendingFluidTicks(level, origin, STAGE, STAGE_HEIGHT, STAGE)));
            line.add("used_stat_delta", el(player.getStats().getValue(net.minecraft.stats.Stats.ITEM_USED.get(held.getItem())) - used));
            line.add("using_item", el(player.isUsingItem()));
            lines.add(line.toString());
            park(player);
            player.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
        }

        void synchronous() {
            for (String fluid : BUCKET_FLUIDS) {
                Block block = fluidBlock(fluid);
                Item bucket = item(fluid + "_bucket");
                BlockState source = block.defaultBlockState();
                for (int pass = 0; pass < 2; pass++) {
                    ServerPlayer player = pass == 0 ? survival : creative;
                    String mode = pass == 0 ? "survival" : "creative";
                    BlockPos floor = abs(3, 0, 3);
                    Runnable atFloor = () -> aim(player, floor, Direction.UP, 2.0);
                    // balde cheio
                    useCase(fluid, player, mode, "full_floor", new ItemStack(bucket), () -> {
                    }, atFloor);
                    useCase(fluid, player, mode, "full_sky", new ItemStack(bucket), () -> {
                    }, () -> aimSky(player));
                    useCase(fluid, player, mode, "full_short_grass", new ItemStack(bucket), () -> {
                        level.setBlock(abs(3, 0, 3), Blocks.GRASS_BLOCK.defaultBlockState(), 2);
                        level.setBlock(abs(3, 1, 3), Blocks.SHORT_GRASS.defaultBlockState(), 2);
                    }, () -> aim(player, abs(3, 1, 3), Direction.UP, 2.0));
                    useCase(fluid, player, mode, "full_tall_grass", new ItemStack(bucket), () -> {
                        level.setBlock(abs(3, 0, 3), Blocks.GRASS_BLOCK.defaultBlockState(), 2);
                        DoublePlantBlock.placeAt(level, Blocks.TALL_GRASS.defaultBlockState(), abs(3, 1, 3), 2);
                    }, () -> aim(player, abs(3, 2, 3), Direction.UP, 2.0));
                    useCase(fluid, player, mode, "full_leaves", new ItemStack(bucket),
                          () -> level.setBlock(abs(3, 1, 3), Blocks.OAK_LEAVES.defaultBlockState().setValue(net.minecraft.world.level.block.LeavesBlock.PERSISTENT, true), 2),
                          () -> aim(player, abs(3, 1, 3), Direction.UP, 2.0));
                    useCase(fluid, player, mode, "full_water_source", new ItemStack(bucket), () -> level.setBlock(abs(3, 1, 3), Blocks.WATER.defaultBlockState(), 2), atFloor);
                    useCase(fluid, player, mode, "full_water_flowing_3", new ItemStack(bucket),
                          () -> level.setBlock(abs(3, 1, 3), Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL, 3), 2), atFloor);
                    useCase(fluid, player, mode, "full_lava_source", new ItemStack(bucket), () -> level.setBlock(abs(3, 1, 3), Blocks.LAVA.defaultBlockState(), 2), atFloor);
                    useCase(fluid, player, mode, "full_own_source", new ItemStack(bucket), () -> level.setBlock(abs(3, 1, 3), source, 2), atFloor);
                    useCase(fluid, player, mode, "full_own_flowing_3", new ItemStack(bucket), () -> level.setBlock(abs(3, 1, 3), source.setValue(LiquidBlock.LEVEL, 3), 2), atFloor);
                    useCase(fluid, player, mode, "full_own_falling_8", new ItemStack(bucket), () -> level.setBlock(abs(3, 1, 3), source.setValue(LiquidBlock.LEVEL, 8), 2), atFloor);
                    // O vidro de 2 de altura em z = 2 entre o jogador (z = 0) e o piso de destino (z = 5): o raio acerta o vidro.
                    useCase(fluid, player, mode, "full_behind_glass", new ItemStack(bucket), () -> {
                        level.setBlock(abs(3, 1, 2), Blocks.GLASS.defaultBlockState(), 2);
                        level.setBlock(abs(3, 2, 2), Blocks.GLASS.defaultBlockState(), 2);
                    }, () -> {
                        Vec3 eye = new Vec3(origin.getX() + 3.5, origin.getY() + 2.5, origin.getZ() + 0.5);
                        player.snapTo(eye.x, eye.y - player.getEyeHeight(), eye.z);
                        player.setDeltaMovement(Vec3.ZERO);
                        player.lookAt(EntityAnchorArgument.Anchor.EYES, new Vec3(origin.getX() + 3.5, origin.getY() + 1.0, origin.getZ() + 5.5));
                    });
                    useCase(fluid, player, mode, "full_side_of_stone", new ItemStack(bucket), () -> level.setBlock(abs(3, 1, 3), Blocks.STONE.defaultBlockState(), 2),
                          () -> aim(player, abs(3, 1, 3), Direction.WEST, 2.0));
                    // balde vazio
                    useCase(fluid, player, mode, "empty_source", new ItemStack(Items.BUCKET), () -> level.setBlock(abs(3, 1, 3), source, 2), atFloor);
                    for (int l : new int[]{1, 2, 3, 4, 5, 6, 7, 8, 9, 15}) {
                        useCase(fluid, player, mode, "empty_level_" + l, new ItemStack(Items.BUCKET), () -> level.setBlock(abs(3, 1, 3), source.setValue(LiquidBlock.LEVEL, l), 2),
                              atFloor);
                    }
                    useCase(fluid, player, mode, "empty_source_from_side", new ItemStack(Items.BUCKET), () -> level.setBlock(abs(3, 1, 3), source, 2),
                          () -> aim(player, abs(3, 1, 3), Direction.NORTH, 1.5));
                }
                dispenser(fluid, bucket, block);
            }
            for (String fluid : BUCKET_FLUIDS) {
                if (fluid.endsWith("nutritional_paste")) {
                    paste(fluid);
                }
            }
            // As marcas de pasta bebida têm ticks: os casos andam um tick por vez em `pasteStep`.
        }

        void dispenser(String fluid, Item bucket, Block block) {
            BlockState source = block.defaultBlockState();
            record Target(String name, BlockState state, BlockState above) {
            }
            List<Target> targets = List.of(new Target("air", Blocks.AIR.defaultBlockState(), null), new Target("short_grass", Blocks.SHORT_GRASS.defaultBlockState(), null),
                  new Target("water_source", Blocks.WATER.defaultBlockState(), null), new Target("own_source", source, null),
                  new Target("own_flowing_3", source.setValue(LiquidBlock.LEVEL, 3), null), new Target("stone", Blocks.STONE.defaultBlockState(), null),
                  new Target("glass", Blocks.GLASS.defaultBlockState(), null), new Target("lava_source", Blocks.LAVA.defaultBlockState(), null));
            for (Target target : targets) {
                reset();
                BlockPos dispenserPos = abs(1, 1, 3);
                BlockPos targetPos = abs(2, 1, 3);
                level.setBlock(dispenserPos, Blocks.DISPENSER.defaultBlockState().setValue(DispenserBlock.FACING, Direction.EAST), 2);
                level.setBlock(abs(2, 0, 3), target.name().equals("short_grass") ? Blocks.GRASS_BLOCK.defaultBlockState() : Blocks.STONE.defaultBlockState(), 2);
                level.setBlock(targetPos, target.state(), 2);
                DispenserBlockEntity entity = (DispenserBlockEntity) level.getBlockEntity(dispenserPos);
                entity.setItem(0, new ItemStack(bucket));
                BlockState[][][] before = snapshot();
                capture.begin(origin);
                level.getBlockState(dispenserPos).tick(level, dispenserPos, level.getRandom());
                JsonObject line = obj("kind", "dispenser", "fluid", fluid, "case", target.name(), "behavior",
                      DispenserBlock.DISPENSER_REGISTRY.get(bucket) == null ? null : DispenserBlock.DISPENSER_REGISTRY.get(bucket).getClass().getName());
                capture.end(line);
                line.add("slot0_after", stack(entity.getItem(0)));
                JsonArray slots = new JsonArray();
                for (int i = 0; i < entity.getContainerSize(); i++) {
                    if (!entity.getItem(i).isEmpty()) {
                        slots.add(arr(i, stack(entity.getItem(i))));
                    }
                }
                line.add("slots_after", slots);
                line.add("cells", diff(before, snapshot()));
                line.add("items", items());
                line.add("fluid_ticks", el(pendingFluidTicks(level, origin, STAGE, STAGE_HEIGHT, STAGE)));
                lines.add(line.toString());
            }
            reset();
        }

        /// O balde de pasta nutritiva: o que o item declara (consumível, tempo de uso) e os casos de beber, que andam tick a tick.
        void paste(String fluid) {
            Item bucket = item(fluid + "_bucket");
            ItemStack stack = new ItemStack(bucket);
            Consumable consumable = stack.get(net.minecraft.core.component.DataComponents.CONSUMABLE);
            lines.add(obj("kind", "paste_item", "fluid", fluid, "class", bucket.getClass().getName(), "useDuration", stack.getUseDuration(survival), "useAnimation", stack.getUseAnimation(),
                  "consumable", consumable == null ? null : obj("consumeSeconds", consumable.consumeSeconds(), "animation", consumable.animation(), "sound",
                        consumable.sound().value().location().toString(), "particles", consumable.hasConsumeParticles(), "effects", consumable.onConsumeEffects().size(),
                        "consumeTicks", consumable.consumeTicks()), "componentKeys", stack.getComponents().keySet().stream()
                        .map(type -> BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(type).toString()).sorted().toList()).toString());
            for (int pass = 0; pass < 2; pass++) {
                ServerPlayer player = pass == 0 ? survival : creative;
                String mode = pass == 0 ? "survival" : "creative";
                for (int food : new int[]{0, 6, 20}) {
                    for (String aimAt : new String[]{"sky", "floor"}) {
                        pasteCases.add(() -> startPaste(fluid, player, mode, food, aimAt));
                    }
                }
            }
        }

        void startPaste(String fluid, ServerPlayer player, String mode, int food, String aimAt) {
            reset();
            pastePlayer = player;
            pasteName = mode + "_food" + food + "_" + aimAt;
            player.getFoodData().setFoodLevel(food);
            player.getFoodData().setSaturation(0.0F);
            if (aimAt.equals("sky")) {
                aimSky(player);
            } else {
                aim(player, abs(3, 0, 3), Direction.UP, 2.0);
            }
            ItemStack held = new ItemStack(item(fluid + "_bucket"));
            player.setItemInHand(InteractionHand.MAIN_HAND, held);
            BlockState[][][] before = snapshot();
            pasteBefore = before;
            capture.begin(origin);
            InteractionResult result = player.gameMode.useItem(player, level, held, InteractionHand.MAIN_HAND);
            JsonObject line = obj("kind", "paste_use", "fluid", fluid, "case", pasteName, "canEat_false", player.canEat(false), "food_before", food, "result", result(result));
            capture.end(line);
            line.add("hand_after", stack(player.getItemInHand(InteractionHand.MAIN_HAND)));
            line.add("using_item", el(player.isUsingItem()));
            line.add("use_ticks_remaining", el(player.getUseItemRemainingTicks()));
            line.add("food_after", el(player.getFoodData().getFoodLevel()));
            line.add("saturation_after", el(player.getFoodData().getSaturationLevel()));
            line.add("cells", diff(before, snapshot()));
            lines.add(line.toString());
            pasteTicks = 0;
        }

        BlockState[][][] pasteBefore;

        /// Um tick de jogo por chamada: começa o próximo caso quando o anterior acabou, e grava o jogador durante a bebida.
        void pasteStep() {
            if (pasteTicks < 0) {
                if (pasteIndex < pasteCases.size()) {
                    pasteCases.get(pasteIndex++).run();
                }
                return;
            }
            pasteTicks++;
            ServerPlayer player = pastePlayer;
            boolean using = player.isUsingItem();
            lines.add(obj("kind", "paste_tick", "case", pasteName, "tick", pasteTicks, "using_item", using, "use_ticks_remaining", player.getUseItemRemainingTicks(),
                  "tickCount", player.tickCount, "food", player.getFoodData().getFoodLevel(), "saturation", player.getFoodData().getSaturationLevel(),
                  "hand", stack(player.getItemInHand(InteractionHand.MAIN_HAND))).toString());
            if (!using && pasteTicks > 2 || pasteTicks >= PASTE_TICKS) {
                JsonObject end = obj("kind", "paste_end", "case", pasteName, "ticks", pasteTicks, "food", player.getFoodData().getFoodLevel(), "saturation", player.getFoodData().getSaturationLevel(),
                      "hand", stack(player.getItemInHand(InteractionHand.MAIN_HAND)), "cells", diff(pasteBefore, snapshot()), "items", items());
                lines.add(end.toString());
                park(player);
                player.setItemInHand(InteractionHand.MAIN_HAND, ItemStack.EMPTY);
                pasteTicks = -1;
            }
        }

        void write() {
            FluidsTraceTest.write("fluids_bucket", lines);
        }
    }

    // ------------------------------------------------------------------------------------------------------------------------------ fluids_flow_*.jsonl

    private record TickRecord(int tick, JsonArray cells, int pending, JsonArray drops) {
    }

    private static final class Flow {
        final String name;
        final int sx;
        final int sy;
        final int sz;
        final int minTicks;
        final int maxTicks;
        final Map<Integer, List<Runnable>> actions = new TreeMap<>();
        final List<TickRecord> records = new ArrayList<>();
        final Set<UUID> seen = new HashSet<>();
        MekGameTestHelper helper;
        ServerLevel level;
        BlockPos origin;
        ServerPlayer player;
        BlockState[] previous;
        boolean recording;
        int tick;
        int lastChange;
        int lastPending = -1;

        Flow(String name, int sx, int sy, int sz, int minTicks, int maxTicks) {
            this.name = name;
            this.sx = sx;
            this.sy = sy;
            this.sz = sz;
            this.minTicks = minTicks;
            this.maxTicks = maxTicks;
        }

        BlockPos abs(int x, int y, int z) {
            return origin.offset(x, y, z);
        }

        /// Sem avisar ninguém: monta o cenário antes de o fluido existir.
        void build(int x, int y, int z, BlockState state) {
            level.setBlock(abs(x, y, z), state, 2);
        }

        void fill(int x1, int y1, int z1, int x2, int y2, int z2, BlockState state) {
            for (int x = x1; x <= x2; x++) {
                for (int y = y1; y <= y2; y++) {
                    for (int z = z1; z <= z2; z++) {
                        build(x, y, z, state);
                    }
                }
            }
        }

        /// Como o balde faz: `setBlockAndUpdate` com o estado padrão (level 0).
        void place(int x, int y, int z, BlockState state) {
            level.setBlockAndUpdate(abs(x, y, z), state);
        }

        void at(int tick, Runnable action) {
            actions.computeIfAbsent(tick, _ -> new ArrayList<>()).add(action);
        }

        ServerPlayer player() {
            if (player == null) {
                player = helper.makeTickingMockServerPlayerInLevel(GameType.SURVIVAL);
                park();
            }
            return player;
        }

        void park() {
            player.snapTo(origin.getX() + 0.5, origin.getY() + 1, origin.getZ() + 0.5);
            player.setDeltaMovement(Vec3.ZERO);
        }

        /// O balde de `BlockItem`/`BucketItem` pelo jogador falso de sobrevivência, mirando de cima o topo do bloco `floor` (relativo).
        void bucket(String item, int x, int y, int z) {
            ServerPlayer p = player();
            Vec3 center = Vec3.atCenterOf(abs(x, y, z));
            Vec3 eye = center.relative(Direction.UP, 2.5);
            p.snapTo(eye.x, eye.y - p.getEyeHeight(), eye.z);
            p.setDeltaMovement(Vec3.ZERO);
            p.lookAt(EntityAnchorArgument.Anchor.EYES, center.relative(Direction.UP, 0.4));
            ItemStack held = new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.parse(item)));
            p.setItemInHand(InteractionHand.MAIN_HAND, held);
            p.gameMode.useItem(p, level, held, InteractionHand.MAIN_HAND);
            park();
        }

        void start(MekGameTestHelper helper, Runnable setup) {
            this.helper = helper;
            this.level = helper.getLevel();
            this.origin = helper.absolutePos(BlockPos.ZERO);
            setup.run();
            previous = snapshot();
            recording = true;
        }

        BlockState[] snapshot() {
            BlockState[] cells = new BlockState[sx * sy * sz];
            int i = 0;
            for (int x = 0; x < sx; x++) {
                for (int y = 0; y < sy; y++) {
                    for (int z = 0; z < sz; z++) {
                        cells[i++] = level.getBlockState(abs(x, y, z));
                    }
                }
            }
            return cells;
        }

        void record() {
            if (!recording) {
                return;
            }
            if (tick > maxTicks) {
                return;
            }
            List<Runnable> now = actions.get(tick);
            if (now != null) {
                now.forEach(Runnable::run);
            }
            BlockState[] current = snapshot();
            JsonArray cells = new JsonArray();
            int i = 0;
            for (int x = 0; x < sx; x++) {
                for (int y = 0; y < sy; y++) {
                    for (int z = 0; z < sz; z++) {
                        BlockState state = current[i];
                        boolean changed = tick == 0 ? !state.isAir() : state != previous[i];
                        if (changed) {
                            cells.add(arr(x, y, z, id(state.getBlock()), levelOf(state)));
                        }
                        i++;
                    }
                }
            }
            previous = current;
            JsonArray drops = new JsonArray();
            AABB box = new AABB(origin.getX(), origin.getY(), origin.getZ(), origin.getX() + sx, origin.getY() + sy, origin.getZ() + sz);
            List<ItemEntity> items = new ArrayList<>(level.getEntitiesOfClass(ItemEntity.class, box));
            items.sort((a, b) -> a.getUUID().compareTo(b.getUUID()));
            List<JsonElement> fresh = new ArrayList<>();
            for (ItemEntity entity : items) {
                if (seen.add(entity.getUUID())) {
                    String itemId = id(entity.getItem().getItem());
                    // A grama solta sementes de trigo ao acaso (12,5 %): a semente não entra, para duas rodadas darem os mesmos bytes.
                    if (!itemId.equals("minecraft:wheat_seeds")) {
                        fresh.add(arr((int) Math.floor(entity.getX()) - origin.getX(), (int) Math.floor(entity.getY()) - origin.getY(),
                              (int) Math.floor(entity.getZ()) - origin.getZ(), itemId, entity.getItem().getCount()));
                    }
                }
            }
            fresh.sort((a, b) -> a.toString().compareTo(b.toString()));
            fresh.forEach(drops::add);
            int pending = pendingFluidTicks(level, origin, sx, sy, sz);
            boolean changed = tick > 0 && (!cells.isEmpty() || pending != lastPending || !drops.isEmpty());
            if (changed) {
                lastChange = tick;
            }
            lastPending = pending;
            records.add(new TickRecord(tick, cells, pending, drops));
            tick++;
        }

        void write() {
            recording = false;
            int end = Math.max(minTicks, Math.min(maxTicks, lastChange + FLOW_STILL_TICKS));
            end = Math.min(end, records.size() - 1);
            List<String> lines = new ArrayList<>();
            for (TickRecord record : records) {
                boolean last = record.tick() == end;
                if (record.tick() > end) {
                    break;
                }
                boolean interesting = record.tick() == 0 || !record.cells().isEmpty() || !record.drops().isEmpty() || last
                      || record.pending() != records.get(record.tick() - 1).pending();
                if (!interesting) {
                    continue;
                }
                JsonObject line = obj("tick", record.tick());
                if (record.tick() == 0) {
                    line.add("full", el(true));
                    line.add("size", arr(sx, sy, sz));
                }
                line.add("cells", record.cells());
                line.add("fluid_ticks", el(record.pending()));
                if (!record.drops().isEmpty()) {
                    line.add("drops", record.drops());
                }
                if (last) {
                    line.add("last", el(true));
                }
                lines.add(line.toString());
            }
            FluidsTraceTest.write("fluids_flow_" + name, lines);
        }
    }

    private interface FlowSetup {
        void run(Flow flow, Block fluid);
    }

    private static void flowTest(DynamicTest test, String name, String fluidId, int sx, int sy, int sz, int minTicks, int maxTicks, FlowSetup setup) {
        template(test, sx, sy, sz);
        Flow flow = new Flow(name, sx, sy, sz, minTicks, maxTicks);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> flow.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> flow.start(helper, () -> setup.run(flow, fluidBlock(fluidId))))
              .thenExecuteAfter(maxTicks + 6, flow::write)
              .thenSucceed());
    }

    private static final String BRINE = "mekanism:brine";

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records a brine source on a stone floor spreading tick by tick.")
    public static void flowFlat(final DynamicTest test) {
        flowTest(test, "flat", BRINE, 31, 4, 31, 0, FLOW_MAX_TICKS, (flow, fluid) -> flow.place(15, 1, 15, fluid.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records a hydrogen source on a stone floor spreading tick by tick.")
    public static void flowFlatHydrogen(final DynamicTest test) {
        flowTest(test, "flat_hydrogen", "mekanism:hydrogen", 31, 4, 31, 0, FLOW_MAX_TICKS, (flow, fluid) -> flow.place(15, 1, 15, fluid.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records a brine source placed by a fake survival player with the bucket.")
    public static void flowFlatBucket(final DynamicTest test) {
        flowTest(test, "flat_bucket", BRINE, 31, 4, 31, 0, FLOW_MAX_TICKS, (flow, fluid) -> flow.bucket("mekanism:brine_bucket", 15, 0, 15));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records brine falling from the corner of a 5 by 5 platform, 6 blocks high.")
    public static void flowWaterfall(final DynamicTest test) {
        flowTest(test, "waterfall", BRINE, 25, 12, 25, 0, FLOW_MAX_TICKS, (flow, fluid) -> {
            flow.fill(10, 1, 10, 14, 6, 14, Blocks.STONE.defaultBlockState());
            flow.place(10, 7, 10, fluid.defaultBlockState());
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records brine on a floor with two 1 block holes and an L shaped wall.")
    public static void flowHole(final DynamicTest test) {
        flowTest(test, "hole", BRINE, 31, 6, 31, 0, FLOW_MAX_TICKS, (flow, fluid) -> {
            BlockState stone = Blocks.STONE.defaultBlockState();
            flow.fill(0, 1, 0, 30, 1, 30, stone);
            flow.build(11, 1, 15, Blocks.AIR.defaultBlockState());
            flow.build(14, 1, 15, Blocks.AIR.defaultBlockState());
            flow.fill(9, 2, 13, 13, 3, 13, stone);
            flow.fill(13, 2, 13, 13, 3, 17, stone);
            flow.place(8, 2, 15, fluid.defaultBlockState());
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records brine running down three stone steps.")
    public static void flowSlope(final DynamicTest test) {
        flowTest(test, "slope", BRINE, 30, 8, 11, 0, FLOW_MAX_TICKS, (flow, fluid) -> {
            BlockState stone = Blocks.STONE.defaultBlockState();
            flow.fill(0, 1, 0, 9, 3, 10, stone);
            flow.fill(10, 1, 0, 19, 2, 10, stone);
            flow.fill(20, 1, 0, 29, 1, 10, stone);
            flow.place(2, 4, 5, fluid.defaultBlockState());
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records brine meeting pillars, a wall with a gap, plants, a torch, a sign, a door and a trapdoor.")
    public static void flowObstacles(final DynamicTest test) {
        flowTest(test, "obstacles", BRINE, 21, 6, 21, 0, FLOW_MAX_TICKS, (flow, fluid) -> {
            BlockState stone = Blocks.STONE.defaultBlockState();
            flow.fill(12, 1, 10, 12, 3, 10, stone);
            flow.fill(10, 1, 13, 10, 3, 13, stone);
            flow.fill(15, 1, 4, 15, 3, 9, stone);
            flow.fill(15, 1, 11, 15, 3, 16, stone);
            // As plantas só sobrevivem sobre terra ou grama.
            flow.build(12, 0, 8, Blocks.GRASS_BLOCK.defaultBlockState());
            flow.build(11, 0, 7, Blocks.GRASS_BLOCK.defaultBlockState());
            flow.build(9, 0, 7, Blocks.GRASS_BLOCK.defaultBlockState());
            flow.build(12, 1, 8, Blocks.SHORT_GRASS.defaultBlockState());
            DoublePlantBlock.placeAt(flow.level, Blocks.TALL_GRASS.defaultBlockState(), flow.abs(11, 1, 7), 2);
            flow.build(9, 1, 12, Blocks.TORCH.defaultBlockState());
            flow.build(12, 1, 12, Blocks.OAK_SIGN.defaultBlockState());
            flow.build(8, 1, 8, Blocks.IRON_DOOR.defaultBlockState());
            flow.build(8, 2, 8, Blocks.IRON_DOOR.defaultBlockState().setValue(BlockStateProperties.DOUBLE_BLOCK_HALF, DoubleBlockHalf.UPPER));
            flow.build(7, 1, 10, Blocks.IRON_TRAPDOOR.defaultBlockState());
            flow.build(9, 1, 7, Blocks.POPPY.defaultBlockState());
            //Folha persistente: a que não é decai por sorteio (random tick) e o rastro mudaria a cada execução.
            flow.build(13, 1, 12, Blocks.OAK_LEAVES.defaultBlockState().setValue(net.minecraft.world.level.block.LeavesBlock.PERSISTENT, true));
            flow.build(13, 1, 8, Blocks.GLASS.defaultBlockState());
            flow.place(10, 1, 10, fluid.defaultBlockState());
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records the flat source replaced by air at tick 60.")
    public static void flowRemoveSource(final DynamicTest test) {
        flowTest(test, "remove_source", BRINE, 31, 4, 31, 180, 180, (flow, fluid) -> {
            flow.place(15, 1, 15, fluid.defaultBlockState());
            flow.at(60, () -> flow.place(15, 1, 15, Blocks.AIR.defaultBlockState()));
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records two pairs of sources 2 and 3 blocks apart, which must not make a new source.")
    public static void flowTwoSources(final DynamicTest test) {
        flowTest(test, "two_sources", BRINE, 31, 4, 31, 0, FLOW_MAX_TICKS, (flow, fluid) -> {
            flow.place(5, 1, 7, fluid.defaultBlockState());
            flow.place(7, 1, 7, fluid.defaultBlockState());
            flow.place(20, 1, 24, fluid.defaultBlockState());
            flow.place(23, 1, 24, fluid.defaultBlockState());
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records a source under a stone ceiling one block above it and a ledge.")
    public static void flowCeiling(final DynamicTest test) {
        flowTest(test, "ceiling", BRINE, 31, 6, 31, 0, FLOW_MAX_TICKS, (flow, fluid) -> {
            BlockState stone = Blocks.STONE.defaultBlockState();
            flow.fill(11, 3, 11, 19, 3, 19, stone);
            flow.fill(20, 2, 14, 22, 2, 16, stone);
            flow.place(15, 1, 15, fluid.defaultBlockState());
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records a source over a 10 block shaft of air and a source covered by stone between ticks 30 and 60.")
    public static void flowColumn(final DynamicTest test) {
        flowTest(test, "column", BRINE, 31, 14, 31, 150, FLOW_MAX_TICKS, (flow, fluid) -> {
            flow.place(6, 11, 15, fluid.defaultBlockState());
            flow.place(24, 1, 15, fluid.defaultBlockState());
            flow.at(30, () -> flow.place(24, 2, 15, Blocks.STONE.defaultBlockState()));
            flow.at(60, () -> flow.place(24, 2, 15, Blocks.AIR.defaultBlockState()));
        });
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records a vanilla lava source alone on the floor, as a control for the lava of meets_water.")
    public static void flowLavaControl(final DynamicTest test) {
        flowTest(test, "lava_control", "minecraft:lava", 31, 4, 31, 0, FLOW_MAX_TICKS, (flow, fluid) -> flow.place(15, 1, 15, fluid.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH_FLOW)
    @TestHolder(description = "Records brine meeting vanilla water and lava, and a water bucket poured over brine.")
    public static void flowMeetsWater(final DynamicTest test) {
        flowTest(test, "meets_water", BRINE, 48, 6, 48, 0, FLOW_MAX_TICKS, (flow, fluid) -> {
            BlockState source = fluid.defaultBlockState();
            flow.place(10, 1, 10, source);
            flow.place(11, 1, 10, Blocks.WATER.defaultBlockState());
            flow.place(34, 1, 10, source);
            flow.at(20, () -> flow.bucket("minecraft:water_bucket", 34, 0, 10));
            flow.place(10, 1, 34, source);
            flow.place(11, 1, 34, Blocks.LAVA.defaultBlockState());
            flow.place(34, 1, 34, source);
            flow.at(25, () -> flow.bucket("minecraft:water_bucket", 37, 0, 34));
        });
    }

    // ------------------------------------------------------------------------------------------------------------------------------ fluids_entity.jsonl

    private record Env(String name, BlockState state, FluidType type) {
    }

    private static final int ENTITY_TICKS = 40;
    private static final int FLOW_ENTITY_TICKS = 20;
    private static final int COLUMN_STEP = 4;
    private static final int COLUMNS_PER_ROW = 6;
    /// Os canais de fluxo: a fonte em x = 7 e o centro do canal em z.
    private static final int CHANNEL_SOURCE_X = 7;
    private static final int[] CHANNEL_Z = {28, 46};
    private static final int CHANNEL_REACH = 8;

    private static final class Run {
        final String env;
        final String entityKind;
        final String start;
        final FluidType type;
        final double x;
        final double y;
        final double z;
        Entity entity;
        final List<String> lines = new ArrayList<>();

        Run(String env, FluidType type, String entityKind, String start, double x, double y, double z) {
            this.env = env;
            this.type = type;
            this.entityKind = entityKind;
            this.start = start;
            this.x = x;
            this.y = y;
            this.z = z;
        }
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records a fake player, an item and a pig dropped in columns of brine, hydrogen, steam, water and air, and standing in flowing brine.")
    public static void fluidsEntity(final DynamicTest test) {
        template(test, 30, 14, 56);
        Entities scene = new Entities();
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.start(helper))
              .thenExecuteAfter(ENTITY_TICKS + FLOW_ENTITY_TICKS + 10, scene::write)
              .thenSucceed());
    }

    private static final class Entities {
        final List<Run> runs = new ArrayList<>();
        final List<Run> flowRuns = new ArrayList<>();
        MekGameTestHelper helper;
        ServerLevel level;
        BlockPos origin;
        boolean recording;
        int age;
        Block brine;

        void start(MekGameTestHelper helper) {
            this.helper = helper;
            this.level = helper.getLevel();
            this.origin = helper.absolutePos(BlockPos.ZERO);
            brine = fluidBlock(BRINE);
            List<Env> envs = List.of(new Env("brine", brine.defaultBlockState(), MekanismFluids.BRINE.getFluidType()),
                  new Env("hydrogen", fluidBlock("mekanism:hydrogen").defaultBlockState(), MekanismFluids.HYDROGEN.getFluidType()),
                  new Env("steam", fluidBlock("mekanism:steam").defaultBlockState(), MekanismFluids.STEAM.getFluidType()),
                  new Env("water", Blocks.WATER.defaultBlockState(), Fluids.WATER.getFluidType()),
                  new Env("air", Blocks.AIR.defaultBlockState(), Fluids.EMPTY.getFluidType()));
            BlockState stone = Blocks.STONE.defaultBlockState();
            int index = 0;
            for (Env env : envs) {
                for (String kind : new String[]{"player", "item", "pig"}) {
                    for (String start : new String[]{"inside", "above"}) {
                        int cx = 2 + COLUMN_STEP * (index % COLUMNS_PER_ROW);
                        int cz = 2 + COLUMN_STEP * (index / COLUMNS_PER_ROW);
                        index++;
                        // Poço de 1 por 1 com paredes de pedra de y = 1 a 10 e 4 fontes de y = 1 a 4.
                        for (int y = 1; y <= 10; y++) {
                            level.setBlock(origin.offset(cx + 1, y, cz), stone, 2);
                            level.setBlock(origin.offset(cx - 1, y, cz), stone, 2);
                            level.setBlock(origin.offset(cx, y, cz + 1), stone, 2);
                            level.setBlock(origin.offset(cx, y, cz - 1), stone, 2);
                        }
                        for (int y = 1; y <= 4; y++) {
                            level.setBlock(origin.offset(cx, y, cz), env.state(), 2);
                        }
                        runs.add(new Run(env.name(), env.type(), kind, start, cx + 0.5, start.equals("inside") ? 3.0 : 9.0, cz + 0.5));
                    }
                }
            }
            for (Run run : runs) {
                run.entity = spawn(run);
                capture(run, 0);
            }
            buildChannels();
            recording = true;
        }

        Entity spawn(Run run) {
            Vec3 pos = new Vec3(origin.getX() + run.x, origin.getY() + run.y, origin.getZ() + run.z);
            switch (run.entityKind) {
                case "player" -> {
                    ServerPlayer player = helper.makeTickingMockServerPlayerInLevel(GameType.SURVIVAL);
                    // O `ServerGamePacketListenerImpl.tickPlayer` devolve o jogador à posição que o cliente mandou (`absSnapTo(firstGood*)`) depois de cada `doTick`,
                    // e o jogador falso não tem cliente: sem a conexão na lista, o jogador anda sozinho como num cliente, e o teste chama `doTick` a cada tick.
                    level.getServer().getConnection().getConnections().remove(player.connection.getConnection());
                    player.snapTo(pos.x, pos.y, pos.z);
                    player.setDeltaMovement(Vec3.ZERO);
                    return player;
                }
                case "item" -> {
                    ItemEntity item = new ItemEntity(level, pos.x, pos.y, pos.z, new ItemStack(Items.COBBLESTONE));
                    item.setDeltaMovement(Vec3.ZERO);
                    item.setId(nextEntityId());
                    level.addFreshEntity(item);
                    return item;
                }
                default -> {
                    Pig pig = EntityTypes.PIG.create(level, EntitySpawnReason.COMMAND);
                    pig.snapTo(pos.x, pos.y, pos.z);
                    pig.setBaby(false);
                    pig.removeFreeWill();
                    pig.setDeltaMovement(Vec3.ZERO);
                    pig.setId(nextEntityId());
                    level.addFreshEntity(pig);
                    return pig;
                }
            }
        }

        void capture(Run run, int tick) {
            Entity e = run.entity;
            Vec3 d = e.getDeltaMovement();
            BlockPos feet = e.blockPosition();
            BlockState feetState = level.getBlockState(feet);
            FluidState feetFluid = feetState.getFluidState();
            JsonObject line = obj("env", run.env, "entity", run.entityKind, "start", run.start, "tick", tick, "x", rnd(e.getX() - origin.getX()), "y", rnd(e.getY() - origin.getY()),
                  "z", rnd(e.getZ() - origin.getZ()), "dx", d.x, "dy", d.y, "dz", d.z, "tickCount", e.tickCount, "isInFluidType", e.isInFluidType(),
                  "isInFluidType_type", e.isInFluidType(run.type), "fluidTypeHeight", e.getFluidTypeHeight(run.type), "isEyeInFluidType", e.isEyeInFluidType(run.type),
                  "maxHeightFluidType", e.getMaxHeightFluidType().toString(), "isSwimming", e.isSwimming(), "isInWater", e.isInWater(), "isUnderWater", e.isUnderWater(),
                  "onGround", e.onGround(), "isInLava", e.isInLava(), "airSupply", e.getAirSupply(), "fallDistance", e.fallDistance, "feet_block", id(feetState.getBlock()),
                  "feet_level", levelOf(feetState), "feet_flow", feetFluid.isEmpty() ? null : feetFluid.getFlow(level, feet));
            if (!(e instanceof ServerPlayer)) {
                line.add("id_mod4", el(e.getId() % 4));
            }
            if (e instanceof LivingEntity living) {
                line.add("health", el(living.getHealth()));
                line.add("isAlive", el(living.isAlive()));
            }
            run.lines.add(line.toString());
        }

        /// Dois canais de fluxo, já espalhando durante os 40 ticks dos poços: o plano e o de degraus (3, 2 e 1 de altura em x).
        void buildChannels() {
            BlockState stone = Blocks.STONE.defaultBlockState();
            for (int variant = 0; variant < 2; variant++) {
                int baseZ = CHANNEL_Z[variant];
                if (variant == 1) {
                    for (int x = 0; x <= 14; x++) {
                        int floorY = x <= 10 ? 3 : x <= 12 ? 2 : 1;
                        for (int z = baseZ - CHANNEL_REACH; z <= baseZ + CHANNEL_REACH; z++) {
                            for (int y = 1; y <= floorY; y++) {
                                level.setBlock(origin.offset(x, y, z), stone, 2);
                            }
                        }
                    }
                }
                level.setBlockAndUpdate(origin.offset(CHANNEL_SOURCE_X, variant == 0 ? 1 : 4, baseZ), brine.defaultBlockState());
            }
        }

        /// Depois de 40 ticks o canal já parou: o jogador entra na célula de `level` 3 a leste da fonte, o item e o porco nas de oeste/sul do mesmo `level`.
        void spawnFlowEntities() {
            FluidType type = MekanismFluids.BRINE.getFluidType();
            for (int variant = 0; variant < 2; variant++) {
                String env = variant == 0 ? "flowing_flat" : "flowing_steps";
                int baseZ = CHANNEL_Z[variant];
                double floor = variant == 0 ? 1.0 : 4.0;
                flowRuns.add(new Run(env, type, "player", "flowing_3", CHANNEL_SOURCE_X + 3.5, floor, baseZ + 0.5));
                flowRuns.add(new Run(env, type, "item", "flowing_3", CHANNEL_SOURCE_X + 0.5, floor, baseZ + 3.5));
                flowRuns.add(new Run(env, type, "pig", "flowing_3", CHANNEL_SOURCE_X + 0.5, floor, baseZ - 2.5));
            }
            for (Run run : flowRuns) {
                run.entity = spawn(run);
                capture(run, 0);
            }
        }

        int ids;

        /// O item parado no chão só se mexe quando `(tickCount + id) % 4 == 0` (`ItemEntity.tick`), e o id vem de um contador do jogo que muda de uma execução para outra: o teste
        /// fixa o id, sempre múltiplo de 4, e o grava (`id_mod4`).
        int nextEntityId() {
            return 1_000_000 + 4 * ids++;
        }

        void tickPlayer(Run run) {
            if (run.entity instanceof ServerPlayer player) {
                player.doTick();
            }
        }

        void record() {
            if (!recording) {
                return;
            }
            age++;
            for (Run run : runs) {
                tickPlayer(run);
            }
            for (Run run : flowRuns) {
                tickPlayer(run);
            }
            if (age <= ENTITY_TICKS) {
                for (Run run : runs) {
                    capture(run, age);
                }
            }
            if (age == ENTITY_TICKS) {
                spawnFlowEntities();
            } else if (age > ENTITY_TICKS && age <= ENTITY_TICKS + FLOW_ENTITY_TICKS) {
                for (Run run : flowRuns) {
                    capture(run, age - ENTITY_TICKS);
                }
            }
        }

        void write() {
            recording = false;
            List<String> lines = new ArrayList<>();
            for (Run run : runs) {
                lines.addAll(run.lines);
            }
            for (Run run : flowRuns) {
                lines.addAll(run.lines);
            }
            FluidsTraceTest.write("fluids_entity", lines);
        }
    }
}
