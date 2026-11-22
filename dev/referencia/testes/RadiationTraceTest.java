package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.api.MekanismRegistries;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.radiation.IRadiationManager;
import mekanism.api.resource.LargeResourceStack;
import mekanism.api.radiation.IRadiationSource;
import mekanism.api.radiation.capability.IRadiationEntity;
import mekanism.common.capabilities.Capabilities;
import mekanism.common.lib.radiation.RadiationLevelData;
import mekanism.common.lib.radiation.RadiationManager;
import mekanism.common.lib.radiation.RadiationScale;
import mekanism.common.lib.radiation.RadiationUtil;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismDamageTypes;
import mekanism.common.registries.MekanismItems;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.TileEntityChemicalTank;
import mekanism.common.tile.TileEntityChemicalTank.GasMode;
import mekanism.common.tile.transmitter.TileEntityPressurizedTube;
import mekanism.common.util.UnitDisplayUtils;
import mekanism.common.util.UnitDisplayUtils.RadiationUnit;
import mekanism.common.util.WorldUtils;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSource;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.SectionPos;
import net.minecraft.gametest.framework.GameTestSequence;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.permissions.LevelBasedPermissionSet;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Mob;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.food.FoodData;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import net.neoforged.neoforge.transfer.ResourceHandler;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// A radiação do original (frente `radiacao`, 1.ª rodada). Cinco rastros em `trace/`:
///
/// - `radiation_rules.jsonl`: um `RadiationLevelData` solto (fora do nível, então o decaimento sorteado do `RadiationManager.onTickPre` não o toca), com
///   `radiate`, o nível e o `maxMagnitude` em pontos a várias distâncias, `decay`, `removeRadiationSource`, `removeRadiationSources`, o NBT (`serialize`) e o
///   NBT relido (`deserialize`); `RadiationScale`, a cor, a severidade, o `getDecayTime`, o `getRadiationDisplay` com 3 casas e o `PreviousRadiationData`.
/// - `radiation_entity.jsonl`: a dose (`RadiationManager.radiate(LivingEntity, m)`) com cada combinação do traje de hazmat, no criativo, em mobs, o
///   `set`, o `decay`, o NBT das attachments e o `update` com dose alta (o dano e a exaustão sorteados).
/// - `radiation_ticks.jsonl`: tick a tick, uma fonte no nível, uma vaca, um husk com máscara e avental e um jogador de teste: a fonte, o nível no
///   bloco de cada um, a dose, a vida e a exaustão.
/// - `radiation_sources.jsonl`: `dumpRadiation` de cada químico, um tubo com polônio quebrado, Chemical Tanks com polônio quebrados (básico e criativo) e
///   despejando, e o item que cai.
/// - `radiation_command.jsonl`: cada subcomando de `/mek radiation`, com as mensagens (`ComponentSerialization.CODEC`) e o valor devolvido.
///
/// Toda fonte posta no nível é tirada no fim de cada teste (`clearSources`), e as entidades são descartadas.
/// Copie o arquivo para `src/src/gameTest/java/mekanism/common/tests/network/` só na hora de rodar o `runGameTestServer` e apague-o depois.
@ForEachTest(groups = "trace")
public class RadiationTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final int TICKS = 400;

    private static void write(String name, List<String> lines) {
        try {
            Path file = Path.of("trace", name);
            Files.createDirectories(file.getParent());
            Files.write(file, lines);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static ChemicalResource chemicalOf(String chemical) {
        ResourceKey<Chemical> key = ResourceKey.create(MekanismRegistries.Keys.CHEMICAL, Identifier.fromNamespaceAndPath("mekanism", chemical));
        return ChemicalResource.of(ServerLifecycleHooks.getCurrentServer().registryAccess().getOrThrow(key));
    }

    private static JsonArray pos(BlockPos pos) {
        JsonArray array = new JsonArray();
        array.add(pos.getX());
        array.add(pos.getY());
        array.add(pos.getZ());
        return array;
    }

    private static JsonElement component(Component component) {
        return ComponentSerialization.CODEC.encodeStart(JsonOps.INSTANCE, component).getOrThrow();
    }

    // ---------------------------------------------------------------- rules

    private static final BlockPos[] SOURCES;
    private static final double[] MAGNITUDES;

    static {
        List<BlockPos> sources = new ArrayList<>();
        List<Double> magnitudes = new ArrayList<>();
        sources.add(new BlockPos(0, 64, 0));
        magnitudes.add(1.0);
        sources.add(new BlockPos(3, 64, 4));
        magnitudes.add(0.5);
        sources.add(new BlockPos(0, 64, 0));
        magnitudes.add(0.25);
        sources.add(new BlockPos(-17, 70, 33));
        magnitudes.add(1.0005E-5);
        sources.add(new BlockPos(-40, -60, -40));
        magnitudes.add(1.0011E-5);
        sources.add(new BlockPos(-1, 64, -1));
        magnitudes.add(3.75);
        //Twenty sources centred in chunk (1, 1): the per chunk hash sets grow past 16 and 32 entries
        for (int i = 0; i < 20; i++) {
            sources.add(new BlockPos(16 + (i % 5) * 3, 64 + i % 3, 16 + (i / 5) * 3));
            magnitudes.add((i + 1) * 0.1 + 1.0E-3 * i);
        }
        sources.add(new BlockPos(300, 64, 300));
        magnitudes.add(123.456);
        SOURCES = sources.toArray(BlockPos[]::new);
        MAGNITUDES = magnitudes.stream().mapToDouble(Double::doubleValue).toArray();
    }

    private static List<BlockPos> probes() {
        List<BlockPos> probes = new ArrayList<>();
        for (BlockPos source : SOURCES) {
            probes.add(source);
        }
        for (int d : new int[]{1, 2, 3, 5, 10, 15, 16, 17, 40, 79, 80, 81, 95, 96, 97}) {
            probes.add(new BlockPos(d, 64, 0));
            probes.add(new BlockPos(-d, 64, 0));
            probes.add(new BlockPos(0, 64 + d, 0));
            probes.add(new BlockPos(d, 64, d));
        }
        for (int x = -120; x <= 140; x += 20) {
            for (int z = -120; z <= 140; z += 20) {
                probes.add(new BlockPos(x, 66, z));
            }
        }
        probes.add(new BlockPos(220, 64, 300));
        probes.add(new BlockPos(379, 64, 300));
        probes.add(new BlockPos(380, 64, 300));
        probes.add(new BlockPos(381, 64, 300));
        return probes;
    }

    private static JsonArray levels(RadiationLevelData data) {
        JsonArray out = new JsonArray();
        for (BlockPos probe : probes()) {
            var lm = data.getRadiationLevelAndMaxMagnitude(probe);
            JsonArray row = pos(probe);
            row.add(lm.level());
            row.add(lm.maxMagnitude());
            out.add(row);
        }
        return out;
    }

    private static CompoundTag save(ServerLevel level, RadiationLevelData data) {
        TagValueOutput output = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, level.registryAccess());
        data.serialize(output);
        return output.buildResult();
    }

    private static String rulesLine(ServerLevel level, String step, RadiationLevelData data) {
        JsonObject line = new JsonObject();
        line.addProperty("step", step);
        line.addProperty("empty", data.isEmpty());
        line.addProperty("nbt", save(level, data).toString());
        line.add("levels", levels(data));
        return line.toString();
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_radiation_rules")
    @TestHolder(description = "Records the radiation rules of a detached RadiationLevelData, the scale, the display and the client sync rule.")
    public static void radiationRules(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(3, 3, 3).fill(0, 0, 0, 2, 0, 2, Blocks.OBSIDIAN.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  ServerLevel level = helper.getLevel();
                  List<String> lines = new ArrayList<>();
                  RadiationLevelData data = new RadiationLevelData();
                  lines.add(rulesLine(level, "empty", data));
                  for (int i = 0; i < SOURCES.length; i++) {
                      data.radiate(SOURCES[i], MAGNITUDES[i]);
                      JsonObject line = new JsonObject();
                      line.addProperty("step", "radiate");
                      line.add("pos", pos(SOURCES[i]));
                      line.addProperty("magnitude", MAGNITUDES[i]);
                      line.addProperty("nbt", save(level, data).toString());
                      lines.add(line.toString());
                  }
                  lines.add(rulesLine(level, "radiated", data));
                  for (int k = 1; k <= 3; k++) {
                      data.decay();
                      lines.add(rulesLine(level, "decay" + k, data));
                  }
                  for (int k = 4; k <= 50; k++) {
                      data.decay();
                  }
                  lines.add(rulesLine(level, "decay50", data));
                  JsonObject removed = new JsonObject();
                  removed.addProperty("step", "remove_at");
                  removed.addProperty("missing", data.removeRadiationSource(new BlockPos(1, 64, 0)));
                  removed.addProperty("hit", data.removeRadiationSource(new BlockPos(3, 64, 4)));
                  lines.add(removed.toString());
                  lines.add(rulesLine(level, "after_remove_at", data));
                  JsonObject chunk = new JsonObject();
                  chunk.addProperty("step", "remove_chunk");
                  chunk.addProperty("empty_chunk", data.removeRadiationSources(5, 5));
                  chunk.addProperty("hit", data.removeRadiationSources(1, 1));
                  lines.add(chunk.toString());
                  lines.add(rulesLine(level, "after_remove_chunk", data));
                  //Saved and loaded again: the order of the list is the order the loaded map hands back
                  CompoundTag saved = save(level, data);
                  RadiationLevelData loaded = new RadiationLevelData();
                  loaded.deserialize(TagValueInput.create(ProblemReporter.DISCARDING, level.registryAccess(), saved));
                  lines.add(rulesLine(level, "reloaded", loaded));
                  //A fresh table with the twenty sources of chunk (1, 1) put in reverse order, saved and loaded
                  RadiationLevelData reverse = new RadiationLevelData();
                  for (int i = SOURCES.length - 1; i >= 0; i--) {
                      reverse.radiate(SOURCES[i], MAGNITUDES[i]);
                  }
                  lines.add(rulesLine(level, "reverse", reverse));
                  RadiationLevelData reverseLoaded = new RadiationLevelData();
                  reverseLoaded.deserialize(TagValueInput.create(ProblemReporter.DISCARDING, level.registryAccess(), save(level, reverse)));
                  lines.add(rulesLine(level, "reverse_reloaded", reverseLoaded));
                  reverse.clearAll();
                  lines.add(rulesLine(level, "cleared", reverse));
                  //Scale, colour, severity, decay time and display
                  double[] values = {0, 1.0E-7, 1.5E-7, 1.0E-6, 9.99E-6, 1.0E-5, 1.0E-4, 5.0E-4, 9.99E-4, 1.0E-3, 0.0123456, 0.05, 0.0999, 0.1, 1, 1.5, 5,
                                     9.999, 10, 50, 99.9, 100, 1000, 1234.5678, 9999.5, 123456.0, 1.0E7};
                  for (double value : values) {
                      JsonObject line = new JsonObject();
                      line.addProperty("step", "scale");
                      line.addProperty("value", value);
                      line.addProperty("scale", RadiationScale.get(value).name());
                      line.addProperty("color", RadiationScale.getSeverityColor(value).name());
                      line.addProperty("severity", RadiationScale.getScaledDoseSeverity(value));
                      line.addProperty("decay_source", RadiationUtil.getDecayTime(value, true));
                      line.addProperty("decay_target", RadiationUtil.getDecayTime(value, false));
                      line.add("display", component(UnitDisplayUtils.getRadiationDisplay(value, RadiationUnit.SVH, 3)));
                      line.add("display_sv", component(UnitDisplayUtils.getRadiationDisplay(value, RadiationUnit.SV, 2)));
                      lines.add(line.toString());
                  }
                  //PreviousRadiationData.compareTo, the rule that decides when the environmental level is synced to the client
                  try {
                      Class<?> previous = Class.forName("mekanism.common.lib.radiation.PlayerExposure$PreviousRadiationData");
                      Method compareTo = previous.getDeclaredMethod("compareTo", previous, double.class);
                      compareTo.setAccessible(true);
                      Method magnitude = previous.getDeclaredMethod("magnitude");
                      magnitude.setAccessible(true);
                      Object last = null;
                      double[] sequence = {1.0E-7, 1.0E-7, 1.05E-7, 1.2E-7, 1.1E-7, 9.9E-8, 9.0E-8, 2.5E-4, 2.5004E-4, 2.51E-4, 2.4999E-4, 9.9E-5, 9.89E-5,
                                           0.5, 0.505, 0.499, 0.0999, 0.09, 1.0E-7, 5.0, 4.99, 4.9, 1234.0, 1233.99, 1233.0, 999.0, 1.0E-7};
                      for (double value : sequence) {
                          Object next = compareTo.invoke(null, last, value);
                          JsonObject line = new JsonObject();
                          line.addProperty("step", "sync");
                          line.addProperty("value", value);
                          line.addProperty("sync", next != null);
                          if (next != null) {
                              last = next;
                          }
                          line.addProperty("kept", last == null ? 0 : (double) magnitude.invoke(last));
                          lines.add(line.toString());
                      }
                  } catch (ReflectiveOperationException e) {
                      throw new IllegalStateException(e);
                  }
                  write("radiation_rules.jsonl", lines);
              })
              .thenSucceed()
        );
    }

    // ---------------------------------------------------------------- entity

    private static IRadiationEntity cap(LivingEntity entity) {
        return entity.getCapability(Capabilities.RADIATION_ENTITY);
    }

    private static void dress(LivingEntity entity, int mask) {
        entity.setItemSlot(EquipmentSlot.HEAD, (mask & 1) != 0 ? new ItemStack(MekanismItems.HAZMAT_MASK.asItem()) : ItemStack.EMPTY);
        entity.setItemSlot(EquipmentSlot.CHEST, (mask & 2) != 0 ? new ItemStack(MekanismItems.HAZMAT_GOWN.asItem()) : ItemStack.EMPTY);
        entity.setItemSlot(EquipmentSlot.LEGS, (mask & 4) != 0 ? new ItemStack(MekanismItems.HAZMAT_PANTS.asItem()) : ItemStack.EMPTY);
        entity.setItemSlot(EquipmentSlot.FEET, (mask & 8) != 0 ? new ItemStack(MekanismItems.HAZMAT_BOOTS.asItem()) : ItemStack.EMPTY);
    }

    private static float exhaustion(ServerPlayer player) {
        try {
            Field field = FoodData.class.getDeclaredField("exhaustionLevel");
            field.setAccessible(true);
            return field.getFloat(player.getFoodData());
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    private static String attachments(LivingEntity entity) {
        TagValueOutput output = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, entity.level().registryAccess());
        entity.saveWithoutId(output);
        CompoundTag tag = output.buildResult();
        return String.valueOf(tag.get("neoforge:attachments"));
    }

    private static ServerPlayer player(MekGameTestHelper helper, GameType type) {
        ServerPlayer player = helper.makeTickingMockServerPlayerInLevel(type);
        helper.getLevel().getServer().getConnection().getConnections().remove(player.connection.getConnection());
        return player;
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_radiation_entity")
    @TestHolder(description = "Records the dose of players and mobs: the hazmat suit, creative mode, set, decay, the saved attachment and the effects of a high dose.")
    public static void radiationEntity(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(9, 4, 9).fill(0, 0, 0, 8, 0, 8, Blocks.OBSIDIAN.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  RadiationManager.get().clearSources();
                  ServerLevel level = helper.getLevel();
                  List<String> lines = new ArrayList<>();
                  ServerPlayer survival = player(helper, GameType.SURVIVAL);
                  ServerPlayer creative = player(helper, GameType.CREATIVE);
                  for (int mask = 0; mask < 16; mask++) {
                      dress(survival, mask);
                      cap(survival).set(IRadiationManager.INSTANCE.baselineRadiation());
                      JsonObject line = new JsonObject();
                      line.addProperty("step", "hazmat");
                      line.addProperty("mask", mask);
                      line.addProperty("resistance", RadiationUtil.getRadiationResistance(survival));
                      RadiationManager.get().radiate(survival, 0.36);
                      line.addProperty("dose", cap(survival).getRadiation());
                      cap(survival).decay();
                      line.addProperty("decayed", cap(survival).getRadiation());
                      lines.add(line.toString());
                  }
                  dress(survival, 0);
                  JsonObject plain = new JsonObject();
                  plain.addProperty("step", "player");
                  cap(survival).set(IRadiationManager.INSTANCE.baselineRadiation());
                  plain.addProperty("attachments_baseline", attachments(survival));
                  cap(survival).set(1.0E-9);
                  plain.addProperty("set_below", cap(survival).getRadiation());
                  cap(survival).radiate(-1);
                  plain.addProperty("radiate_negative", cap(survival).getRadiation());
                  cap(survival).radiate(0.002);
                  plain.addProperty("cap_radiate", cap(survival).getRadiation());
                  RadiationManager.get().radiate(survival, 0.001);
                  plain.addProperty("manager_radiate", cap(survival).getRadiation());
                  plain.addProperty("attachments", attachments(survival));
                  double decayed = cap(survival).getRadiation();
                  JsonArray decays = new JsonArray();
                  for (int i = 0; i < 5; i++) {
                      cap(survival).decay();
                      decays.add(cap(survival).getRadiation());
                  }
                  plain.add("decays", decays);
                  cap(creative).set(IRadiationManager.INSTANCE.baselineRadiation());
                  RadiationManager.get().radiate(creative, 0.5);
                  plain.addProperty("creative_manager", cap(creative).getRadiation());
                  cap(creative).radiate(0.5);
                  plain.addProperty("creative_cap", cap(creative).getRadiation());
                  lines.add(plain.toString());
                  //Mobs
                  List<EntityType<? extends LivingEntity>> types = List.of(EntityTypes.COW, EntityTypes.HUSK, EntityTypes.IRON_GOLEM, EntityTypes.SNOW_GOLEM);
                  for (EntityType<? extends LivingEntity> type : types) {
                      LivingEntity mob = helper.spawn(type, new BlockPos(4, 1, 4));
                      if (mob instanceof Mob m) {
                          m.setNoAi(true);
                      }
                      if (type == EntityTypes.HUSK) {
                          dress(mob, 3);
                      }
                      JsonObject line = new JsonObject();
                      line.addProperty("step", "mob");
                      line.addProperty("type", EntityType.getKey(type).toString());
                      line.addProperty("has_cap", cap(mob) != null);
                      line.addProperty("resistance", RadiationUtil.getRadiationResistance(mob));
                      RadiationManager.get().radiate(mob, 0.01);
                      line.addProperty("dose", cap(mob).getRadiation());
                      float before = mob.getHealth();
                      line.addProperty("hurt", mob.hurtServer(level, MekanismDamageTypes.RADIATION.source(level), 2));
                      line.addProperty("health_lost", before - mob.getHealth());
                      lines.add(line.toString());
                      mob.discard();
                  }
                  //A high dose: update() rolls the effects (the chance, then a coin for the damage); 40 calls with the hurt cooldown cleared before each
                  for (double dose : new double[]{1.0E-5, 5.0E-5, 0.001, 0.05, 2.0, 50.0}) {
                      for (int i = 0; i < 40; i++) {
                          survival.setHealth(survival.getMaxHealth());
                          survival.setInvulnerableTime(0);
                          survival.getFoodData().setFoodLevel(20);
                          float exhaustion = exhaustion(survival);
                          cap(survival).set(dose);
                          cap(survival).update();
                          JsonObject line = new JsonObject();
                          line.addProperty("step", "update");
                          line.addProperty("dose", dose);
                          line.addProperty("severity", RadiationScale.getScaledDoseSeverity(dose));
                          line.addProperty("health_lost", survival.getMaxHealth() - survival.getHealth());
                          line.addProperty("exhaustion_added", exhaustion(survival) - exhaustion);
                          line.addProperty("dose_after", cap(survival).getRadiation());
                          lines.add(line.toString());
                      }
                  }
                  cap(survival).set(IRadiationManager.INSTANCE.baselineRadiation());
                  survival.setHealth(survival.getMaxHealth());
                  write("radiation_entity.jsonl", lines);
              })
              .thenSucceed()
        );
    }

    // ---------------------------------------------------------------- ticks

    private static final class TickState {

        final List<String> lines = new ArrayList<>();
        BlockPos source;
        LivingEntity cow;
        LivingEntity husk;
        ServerPlayer player;
        int tick;
    }

    private static JsonObject entityState(ServerLevel level, LivingEntity entity) {
        JsonObject out = new JsonObject();
        out.add("pos", pos(entity.blockPosition()));
        out.addProperty("level", RadiationManager.get().getRadiationLevel(level, entity.blockPosition()));
        out.addProperty("dose", cap(entity) == null ? -1 : cap(entity).getRadiation());
        out.addProperty("health", entity.getHealth());
        out.addProperty("alive", entity.isAlive());
        return out;
    }

    private static void recordTick(MekGameTestHelper helper, TickState state) {
        ServerLevel level = helper.getLevel();
        JsonObject line = new JsonObject();
        line.addProperty("tick", state.tick++);
        line.addProperty("game_time", level.getGameTime());
        JsonArray sources = new JsonArray();
        for (IRadiationSource source : RadiationManager.get().getRadiationSources(level, SectionPos.blockToSectionCoord(state.source.getX()),
              SectionPos.blockToSectionCoord(state.source.getZ()))) {
            JsonArray row = pos(source.getPosition());
            row.add(source.getMagnitude());
            sources.add(row);
        }
        line.add("sources", sources);
        line.add("cow", entityState(level, state.cow));
        line.add("husk", entityState(level, state.husk));
        JsonObject player = entityState(level, state.player);
        player.addProperty("exhaustion", exhaustion(state.player));
        line.add("player", player);
        state.lines.add(line.toString());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_radiation_ticks")
    @TestHolder(description = "Records, tick by tick, a radiation source in the level, a cow, a husk with a hazmat mask and gown and a mock player near it.")
    public static void radiationTicks(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(11, 4, 11).fill(0, 0, 0, 10, 0, 10, Blocks.OBSIDIAN.defaultBlockState()));
        final TickState state = new TickState();
        test.onGameTest(MekGameTestHelper.class, helper -> {
            GameTestSequence sequence = helper.startSequence().thenExecute(() -> {
                ServerLevel level = helper.getLevel();
                RadiationManager.get().clearSources();
                state.source = helper.absolutePos(new BlockPos(1, 1, 1));
                state.cow = helper.spawn(EntityTypes.COW, new BlockPos(4, 1, 1));
                ((Mob) state.cow).setNoAi(true);
                state.husk = helper.spawn(EntityTypes.HUSK, new BlockPos(1, 1, 4));
                ((Mob) state.husk).setNoAi(true);
                dress(state.husk, 3);
                state.player = player(helper, GameType.SURVIVAL);
                BlockPos at = helper.absolutePos(new BlockPos(7, 1, 1));
                state.player.snapTo(at.getX() + 0.5, at.getY(), at.getZ() + 0.5);
                state.player.setDeltaMovement(Vec3.ZERO);
                cap(state.player).set(IRadiationManager.INSTANCE.baselineRadiation());
                RadiationManager.get().radiate(level, state.source, 20);
                recordTick(helper, state);
            });
            for (int i = 0; i < TICKS; i++) {
                sequence = sequence.thenIdle(1).thenExecute(() -> recordTick(helper, state));
            }
            sequence.thenExecute(() -> {
                      RadiationManager.get().clearSources();
                      cap(state.player).set(IRadiationManager.INSTANCE.baselineRadiation());
                      state.player.setHealth(state.player.getMaxHealth());
                      state.cow.discard();
                      state.husk.discard();
                      write("radiation_ticks.jsonl", state.lines);
                  })
                  .thenSucceed();
        });
    }

    // ---------------------------------------------------------------- sources

    private static JsonArray sourcesAround(ServerLevel level, BlockPos center) {
        JsonArray out = new JsonArray();
        int cx = SectionPos.blockToSectionCoord(center.getX());
        int cz = SectionPos.blockToSectionCoord(center.getZ());
        for (int x = cx - 1; x <= cx + 1; x++) {
            for (int z = cz - 1; z <= cz + 1; z++) {
                for (IRadiationSource source : RadiationManager.get().getRadiationSources(level, x, z)) {
                    JsonArray row = pos(source.getPosition());
                    row.add(source.getMagnitude());
                    out.add(row);
                }
            }
        }
        return out;
    }

    private static JsonArray drops(ServerLevel level, AABB box) {
        RegistryOps<JsonElement> json = RegistryOps.create(JsonOps.INSTANCE, level.registryAccess());
        JsonArray out = new JsonArray();
        for (ItemEntity entity : level.getEntitiesOfClass(ItemEntity.class, box)) {
            out.add(ItemStack.CODEC.encodeStart(json, entity.getItem()).getOrThrow());
            entity.discard();
        }
        return out;
    }

    private static final class SourceState {

        final List<String> lines = new ArrayList<>();
    }

    private static void line(SourceState state, String step, ServerLevel level, BlockPos center, JsonObject extra) {
        extra.addProperty("step", step);
        extra.addProperty("game_time", level.getGameTime());
        extra.add("sources", sourcesAround(level, center));
        state.lines.add(extra.toString());
    }

    private static TileEntityChemicalTank tank(MekGameTestHelper helper, BlockPos rel, Block block, String chemical, long amount) {
        BlockPos pos = helper.absolutePos(rel);
        helper.getLevel().setBlockAndUpdate(pos, block.defaultBlockState());
        TileEntityChemicalTank tile = WorldUtils.getTileEntity(TileEntityChemicalTank.class, helper.getLevel(), pos);
        tile.getChemicalTank().setContents(chemicalOf(chemical), amount, null);
        return tile;
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_radiation_sources")
    @TestHolder(description = "Records the radiation sources left by dumpRadiation, a broken pressurized tube with polonium and broken or dumping chemical tanks.")
    public static void radiationSources(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(9, 4, 9).fill(0, 0, 0, 8, 0, 8, Blocks.OBSIDIAN.defaultBlockState()));
        final SourceState state = new SourceState();
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  ServerLevel level = helper.getLevel();
                  RadiationManager.get().clearSources();
                  BlockPos center = helper.absolutePos(new BlockPos(4, 1, 4));
                  String[] chemicals = {"polonium", "plutonium", "nuclear_waste", "spent_nuclear_waste", "hydrogen", "polonium", "polonium"};
                  long[] amounts = {1000, 1000, 1000, 1, 1000, 0, 7};
                  for (int i = 0; i < chemicals.length; i++) {
                      BlockPos at = center.offset(i - 3, 0, 0);
                      JsonObject extra = new JsonObject();
                      extra.addProperty("chem", chemicals[i]);
                      extra.addProperty("amount", amounts[i]);
                      extra.add("pos", pos(at));
                      extra.addProperty("radioactivity", chemicalOf(chemicals[i]).getRadioactivity(level.registryAccess()));
                      extra.addProperty("dumped", RadiationManager.get().dumpRadiation(level, at, chemicalOf(chemicals[i]), amounts[i]));
                      line(state, "dump", level, center, extra);
                  }
                  RadiationManager.get().clearSources();
                  //A lone basic tube
                  level.setBlockAndUpdate(helper.absolutePos(new BlockPos(2, 1, 2)), MekanismBlocks.BASIC_PRESSURIZED_TUBE.defaultState());
              })
              .thenIdle(2)
              .thenExecute(() -> {
                  ServerLevel level = helper.getLevel();
                  BlockPos pos = helper.absolutePos(new BlockPos(2, 1, 2));
                  ResourceHandler<ChemicalResource> handler = level.getCapability(Capabilities.CHEMICAL.block(), pos, Direction.UP);
                  int got = 0;
                  if (handler != null) {
                      try (Transaction transaction = Transaction.openRoot()) {
                          got = handler.insert(chemicalOf("polonium"), 1000, transaction);
                          transaction.commit();
                      }
                  }
                  JsonObject extra = new JsonObject();
                  extra.addProperty("got", got);
                  line(state, "tube_push", level, pos, extra);
              })
              .thenIdle(3)
              .thenExecute(() -> {
                  ServerLevel level = helper.getLevel();
                  BlockPos pos = helper.absolutePos(new BlockPos(2, 1, 2));
                  TileEntityPressurizedTube tile = WorldUtils.getTileEntity(TileEntityPressurizedTube.class, level, pos);
                  JsonObject extra = new JsonObject();
                  extra.addProperty("scale", tile == null ? -1 : tile.getRadiationScale(level));
                  extra.addProperty("particles", tile == null ? -1 : tile.getRadiationParticleCount(level));
                  extra.addProperty("destroyed", level.destroyBlock(pos, false));
                  line(state, "tube_break", level, pos, extra);
              })
              .thenIdle(2)
              .thenExecute(() -> {
                  ServerLevel level = helper.getLevel();
                  BlockPos pos = helper.absolutePos(new BlockPos(2, 1, 2));
                  line(state, "tube_after", level, pos, new JsonObject());
                  RadiationManager.get().clearSources();
                  tank(helper, new BlockPos(2, 1, 2), MekanismBlocks.BASIC_CHEMICAL_TANK.get(), "polonium", 1000);
                  tank(helper, new BlockPos(6, 1, 2), MekanismBlocks.CREATIVE_CHEMICAL_TANK.get(), "polonium", 1000);
                  tank(helper, new BlockPos(2, 1, 6), MekanismBlocks.BASIC_CHEMICAL_TANK.get(), "hydrogen", 1000);
                  TileEntityChemicalTank dumping = tank(helper, new BlockPos(6, 1, 6), MekanismBlocks.BASIC_CHEMICAL_TANK.get(), "polonium", 1000);
                  dumping.dumping = GasMode.DUMPING;
              })
              .thenIdle(3)
              .thenExecute(() -> {
                  ServerLevel level = helper.getLevel();
                  AABB box = new AABB(helper.absolutePos(BlockPos.ZERO)).minmax(new AABB(helper.absolutePos(new BlockPos(8, 3, 8)))).inflate(4);
                  drops(level, box);
                  for (BlockPos rel : new BlockPos[]{new BlockPos(2, 1, 2), new BlockPos(6, 1, 2), new BlockPos(2, 1, 6), new BlockPos(6, 1, 6)}) {
                      BlockPos pos = helper.absolutePos(rel);
                      TileEntityChemicalTank tile = WorldUtils.getTileEntity(TileEntityChemicalTank.class, level, pos);
                      JsonObject extra = new JsonObject();
                      extra.add("tank", pos(rel));
                      extra.addProperty("amount", tile.getChemicalTank().amountAsLong());
                      extra.addProperty("scale", tile.getRadiationScale(level));
                      extra.addProperty("particles", tile.getRadiationParticleCount(level));
                      extra.addProperty("mode", tile.dumping.name());
                      if (tile.dumping == GasMode.IDLE) {
                          extra.addProperty("destroyed", level.destroyBlock(pos, true));
                          extra.add("drops", drops(level, box));
                      }
                      line(state, "tank", level, pos, extra);
                  }
              })
              .thenIdle(20)
              .thenExecute(() -> {
                  ServerLevel level = helper.getLevel();
                  BlockPos pos = helper.absolutePos(new BlockPos(6, 1, 6));
                  TileEntityChemicalTank tile = WorldUtils.getTileEntity(TileEntityChemicalTank.class, level, pos);
                  JsonObject extra = new JsonObject();
                  extra.addProperty("amount", tile.getChemicalTank().amountAsLong());
                  extra.addProperty("scale", tile.getRadiationScale(level));
                  line(state, "tank_dumping", level, pos, extra);
                  tile.getChemicalTank().setContents(LargeResourceStack.CHEMICAL_HELPER.empty(), null);
                  level.removeBlock(pos, false);
                  RadiationManager.get().clearSources();
                  write("radiation_sources.jsonl", state.lines);
              })
              .thenSucceed()
        );
    }

    // ---------------------------------------------------------------- command

    private static final class Collector implements CommandSource {

        final List<Component> messages = new ArrayList<>();

        @Override
        public void sendSystemMessage(Component message) {
            messages.add(message);
        }

        @Override
        public boolean acceptsSuccess() {
            return true;
        }

        @Override
        public boolean acceptsFailure() {
            return true;
        }

        @Override
        public boolean shouldInformAdmins() {
            return false;
        }
    }

    private static String command(ServerPlayer player, String command) {
        Collector collector = new Collector();
        int[] result = {Integer.MIN_VALUE};
        boolean[] success = {false};
        CommandSourceStack stack = player.createCommandSourceStack().withPermission(LevelBasedPermissionSet.OWNER).withSource(collector)
              .withCallback((ok, value) -> {
                  success[0] = ok;
                  result[0] = value;
              });
        player.level().getServer().getCommands().performPrefixedCommand(stack, command);
        JsonObject line = new JsonObject();
        line.addProperty("command", command);
        line.addProperty("success", success[0]);
        line.addProperty("result", result[0]);
        JsonArray messages = new JsonArray();
        for (Component message : collector.messages) {
            messages.add(component(message));
        }
        line.add("messages", messages);
        line.addProperty("dose", cap(player).getRadiation());
        return line.toString();
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_radiation_command")
    @TestHolder(description = "Records every /mek radiation sub command: the messages, the returned value and the player's dose.")
    public static void radiationCommand(final DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(9, 4, 9).fill(0, 0, 0, 8, 0, 8, Blocks.OBSIDIAN.defaultBlockState()));
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  ServerLevel level = helper.getLevel();
                  RadiationManager.get().clearSources();
                  List<String> lines = new ArrayList<>();
                  ServerPlayer player = player(helper, GameType.SURVIVAL);
                  BlockPos at = helper.absolutePos(new BlockPos(4, 1, 4));
                  player.snapTo(at.getX() + 0.5, at.getY(), at.getZ() + 0.5);
                  cap(player).set(IRadiationManager.INSTANCE.baselineRadiation());
                  LivingEntity cow = helper.spawn(EntityTypes.COW, new BlockPos(6, 1, 6));
                  ((Mob) cow).setNoAi(true);
                  cow.setCustomName(Component.literal("Mimosa"));
                  JsonObject where = new JsonObject();
                  where.addProperty("step", "where");
                  where.add("player", pos(player.blockPosition()));
                  where.add("cow", pos(cow.blockPosition()));
                  lines.add(where.toString());
                  int x = at.getX(), y = at.getY(), z = at.getZ();
                  String[] commands = {
                        "mek radiation get",
                        "mek radiation add 0.5",
                        "mek radiation get",
                        "mek radiation add 0.000002 " + (x + 3) + " " + y + " " + z,
                        "mek radiation add 2500 " + (x + 1.7) + " " + (y + 0.2) + " " + (z - 0.4) + " minecraft:overworld",
                        "mek radiation get " + (x + 2) + " " + y + " " + z,
                        "mek radiation get " + (x + 40) + " " + y + " " + z + " minecraft:overworld",
                        "mek radiation get " + (x + 200) + " " + y + " " + z,
                        "mek radiation addEntity 0.002",
                        "mek radiation addEntity 0.0000001",
                        "mek radiation addEntity @e[type=minecraft:cow,distance=..20] 0.75",
                        "mek radiation addEntity @e[distance=..20] 0.003",
                        "mek radiation reduce 0.0005",
                        "mek radiation reduce 5",
                        "mek radiation reduce @e[type=minecraft:cow,distance=..20] 0.25",
                        "mek radiation reduce @e[distance=..20] 10",
                        "mek radiation addEntity 1.5",
                        "mek radiation heal",
                        "mek radiation addEntity @e[distance=..20] 0.5",
                        "mek radiation heal @e[type=minecraft:cow,distance=..20]",
                        "mek radiation heal @e[distance=..20]",
                        "mek radiation add 0",
                        "mek radiation add 20000",
                        "mek radiation removeAll",
                        "mek radiation get",
                  };
                  for (String command : commands) {
                      lines.add(command(player, command));
                  }
                  RadiationManager.get().clearSources();
                  cap(player).set(IRadiationManager.INSTANCE.baselineRadiation());
                  cow.discard();
                  write("radiation_command.jsonl", lines);
              })
              .thenSucceed()
        );
    }
}
