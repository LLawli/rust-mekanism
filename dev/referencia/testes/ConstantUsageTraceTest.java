package mekanism.common.tests.network;

import it.unimi.dsi.fastutil.objects.Object2IntMap;
import it.unimi.dsi.fastutil.objects.Object2IntOpenHashMap;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.api.MekanismRegistries;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.recipes.cache.CachedRecipe.OperationTracker.RecipeError;
import mekanism.api.resource.LargeResourceStack;
import mekanism.api.upgrade.Upgrade;
import mekanism.api.upgrade.UpgradeIds;
import mekanism.common.block.attribute.Attribute;
import mekanism.common.capabilities.energy.MachineEnergyContainer;
import mekanism.common.component.component.UpgradeAware;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.registries.MekanismDataComponents;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava o Metallurgic Infuser com uma receita `metallurgic_infusing` de `per_tick_usage` (`ItemStackConstantChemicalToObjectCachedRecipe`):
/// gravel e 2 de redstone por tick, sem receita igual nos arquivos do original. A receita vem de um data pack do próprio teste:
/// `dev/referencia/testes/dados/data/mekanismtests/recipe/metallurgic_infusing/gravel_to_flint_per_tick.json` vai para
/// `src/gameTest/resources/data/mekanismtests/recipe/metallurgic_infusing/` só na hora de rodar o `runGameTestServer`.
/// A máquina é posta por um jogador (`useOn` com o item, que passa por `applyImplicitComponents`), com a carga de energia que um cabo
/// manteria (o mesmo `refill` do `MachineTraceTest`). `infuser_const_<cenário>.jsonl`, um tick por linha, com `used_so_far` e
/// `max_operations` além dos campos de `infuser_run`.
@ForEachTest(groups = "trace")
public class ConstantUsageTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final BlockPos FLOOR = new BlockPos(3, 0, 1);
    private static final BlockPos SPOT = new BlockPos(3, 1, 1);
    private static final int INFUSION = 0;
    private static final int INPUT = 1;
    private static final int OUTPUT = 2;
    private static final int REFILL_TICKS = 3;
    /// `maxUpgradeMultiplier` dos cenários de mais de uma operação por tick: o padrão (10) deixa o Infuser em `200 / 10 = 20` ticks, e
    /// `UpgradeUtils.getOperationsPerTick` só passa de 1 com `ticksD < 1`.
    private static final int OPS_MULTIPLIER = 1000;

    private static final List<RecipeError> ERRORS = List.of(RecipeError.NOT_ENOUGH_ENERGY, RecipeError.NOT_ENOUGH_INPUT,
          RecipeError.NOT_ENOUGH_OUTPUT_SPACE, RecipeError.INPUT_DOESNT_PRODUCE_OUTPUT, RecipeError.NOT_ENOUGH_SECONDARY_INPUT);
    private static final List<String> ERROR_NAMES = List.of("energy", "input", "output_space", "wrong_output", "secondary");

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(6, 3, 3).fill(3, 0, 1, 3, 0, 1, Blocks.STONE.defaultBlockState()));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_constant")
    @TestHolder(description = "Records the Metallurgic Infuser spending the chemical of a per_tick_usage recipe: two full operations with chemical left over, then a third that runs dry and resumes.")
    public static void infuserConstRun(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("infuser_const_run");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper, 0, 0))
              .thenExecuteAfter(5, () -> scene.apply("item gravel 2, tank redstone 1000"))
              .thenExecuteAfter(440, () -> scene.apply("item gravel 1"))
              .thenExecuteAfter(150, () -> scene.apply("tank redstone 500"))
              .thenExecuteAfter(150, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_constant")
    @TestHolder(description = "Records the Metallurgic Infuser with a per_tick_usage recipe when the chemical runs out in the middle of an operation, comes back, is less than one recipe amount, is another chemical, and when the input is removed.")
    public static void infuserConstDry(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("infuser_const_dry");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper, 0, 0))
              .thenExecuteAfter(5, () -> scene.apply("item gravel 3, tank redstone 150"))
              .thenExecuteAfter(100, () -> scene.apply("tank redstone 1"))
              .thenExecuteAfter(20, () -> scene.apply("tank redstone 100"))
              .thenExecuteAfter(80, () -> scene.apply("tank bio 100"))
              .thenExecuteAfter(30, () -> scene.apply("tank redstone 1000"))
              .thenExecuteAfter(120, () -> scene.apply("item gravel 0"))
              .thenExecuteAfter(20, () -> scene.apply("item gravel 3"))
              .thenExecuteAfter(100, () -> scene.apply("tank redstone 0"))
              .thenExecuteAfter(30, () -> scene.apply("tank redstone 1000"))
              .thenExecuteAfter(250, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_constant")
    @TestHolder(description = "Records the Metallurgic Infuser with a per_tick_usage recipe while speed and energy upgrades are installed and removed in the middle of the operation.")
    public static void infuserConstSpeed(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("infuser_const_speed");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper, 0, 0))
              .thenExecuteAfter(5, () -> scene.apply("item gravel 4, tank redstone 1000"))
              .thenExecuteAfter(60, () -> scene.apply("speed 3"))
              .thenExecuteAfter(30, () -> scene.apply("energy 2"))
              .thenExecuteAfter(30, () -> scene.apply("tank redstone 1000"))
              .thenExecuteAfter(20, () -> scene.apply("speed 0"))
              .thenExecuteAfter(80, () -> scene.apply("tank redstone 1000"))
              .thenExecuteAfter(100, () -> scene.apply("speed 8"))
              .thenExecuteAfter(60, () -> scene.apply("tank redstone 1000"))
              .thenExecuteAfter(30, () -> scene.apply("speed 6"))
              .thenExecuteAfter(100, () -> scene.apply("speed 1"))
              .thenExecuteAfter(120, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_constant")
    @TestHolder(description = "Records the Metallurgic Infuser with a per_tick_usage recipe placed by a player with 4 speed and 1 energy upgrades in the item: two operations, then the chemical running dry.")
    public static void infuserConstPlacedSpeed(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("infuser_const_placed_speed");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper, 4, 1))
              .thenExecuteAfter(5, () -> scene.apply("item gravel 3, tank redstone 600"))
              .thenExecuteAfter(200, () -> scene.apply("tank redstone 700"))
              .thenExecuteAfter(100, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_constant")
    @TestHolder(description = "Records the Metallurgic Infuser with a per_tick_usage recipe saved and loaded again in the middle of the operation, with upgrades and while paused for lack of chemical.")
    public static void infuserConstReload(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("infuser_const_reload");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper, 0, 0))
              .thenExecuteAfter(5, () -> scene.apply("item gravel 3, tank redstone 1000"))
              .thenExecuteAfter(90, () -> scene.apply("reload"))
              .thenExecuteAfter(60, () -> scene.apply("reload"))
              .thenExecuteAfter(100, () -> scene.apply("speed 3"))
              .thenExecuteAfter(20, () -> scene.apply("reload"))
              .thenExecuteAfter(40, () -> scene.apply("tank redstone 0"))
              .thenExecuteAfter(10, () -> scene.apply("reload"))
              .thenExecuteAfter(10, () -> scene.apply("tank redstone 1000"))
              .thenExecuteAfter(300, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_constant")
    @TestHolder(description = "Records the Metallurgic Infuser with a per_tick_usage recipe saved with a progress that no longer fits the loaded speed upgrades: the saved usedSoFar is loaded and the saved progress is not.")
    public static void infuserConstReloadSpeed(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("infuser_const_reload_speed");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper, 0, 0))
              .thenExecuteAfter(5, () -> scene.apply("item gravel 6, tank redstone 1000"))
              .thenExecuteAfter(100, () -> scene.apply("speed 8, reload"))
              .thenExecuteAfter(50, () -> scene.apply("speed 0"))
              .thenExecuteAfter(30, () -> scene.apply("speed 5, reload"))
              .thenExecuteAfter(30, () -> scene.apply("speed 8, reload"))
              .thenExecuteAfter(100, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_constant")
    @TestHolder(description = "Records the Metallurgic Infuser with a per_tick_usage recipe loaded from a save that has the progress but not used_so_far: the multiplier catches up with more than one per tick.")
    public static void infuserConstReloadWithoutUsage(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("infuser_const_reload_without_usage");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper, 0, 0))
              .thenExecuteAfter(5, () -> scene.apply("item gravel 2, tank redstone 1000"))
              .thenExecuteAfter(100, () -> scene.apply("reload-without-usage"))
              .thenExecuteAfter(200, () -> scene.apply("tank redstone 1000"))
              .thenExecuteAfter(150, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_constant_ops_a")
    @TestHolder(description = "Records the Metallurgic Infuser with a per_tick_usage recipe and maxUpgradeMultiplier 1000, where speed 7 and 8 make getOperationsPerTick pass 1 and the chemical use per tick multiplies.")
    public static void infuserConstOps(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("infuser_const_ops");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper, 0, 0, OPS_MULTIPLIER))
              .thenExecuteAfter(5, () -> scene.apply("item gravel 40, tank redstone 3000"))
              .thenExecuteAfter(60, () -> scene.apply("speed 4"))
              .thenExecuteAfter(40, () -> scene.apply("speed 7"))
              .thenExecuteAfter(40, () -> scene.apply("energy 3"))
              .thenExecuteAfter(30, () -> scene.apply("speed 8"))
              .thenExecuteAfter(30, () -> scene.apply("tank redstone 3000"))
              .thenExecuteAfter(40, () -> scene.apply("speed 6"))
              .thenExecuteAfter(30, () -> scene.apply("speed 7, reload"))
              .thenExecuteAfter(40, () -> scene.apply("tank redstone 5"))
              .thenExecuteAfter(20, () -> scene.apply("tank redstone 3000"))
              .thenExecuteAfter(40, () -> scene.apply("speed 0"))
              .thenExecuteAfter(60, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_constant_ops_b")
    @TestHolder(description = "Records the Metallurgic Infuser with a per_tick_usage recipe and maxUpgradeMultiplier 1000 placed by a player with 8 speed and 2 energy upgrades in the item, the chemical running dry in the middle of the operations.")
    public static void infuserConstOpsPlaced(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("infuser_const_ops_placed");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> scene.begin(helper, 8, 2, OPS_MULTIPLIER))
              .thenExecuteAfter(5, () -> scene.apply("item gravel 30, tank redstone 120"))
              .thenExecuteAfter(40, () -> scene.apply("tank redstone 3000"))
              .thenExecuteAfter(40, () -> scene.apply("speed 7, tank redstone 40"))
              .thenExecuteAfter(30, () -> scene.apply("tank redstone 3000"))
              .thenExecuteAfter(40, () -> scene.apply("reload"))
              .thenExecuteAfter(60, scene::write)
              .thenSucceed());
    }

    private static final class Scene {
        private final String name;
        private final List<String> lines = new ArrayList<>();
        private final List<String> marks = new ArrayList<>();
        private MekGameTestHelper helper;
        private int tick;

        private boolean multiplierChanged;

        Scene(String name) {
            this.name = name;
        }

        /// O valor em cache de `CachedIntValue` (sem gravar o arquivo de configuração): é ele que `UpgradeUtils.getTicksD` lê.
        private static void setMultiplier(int value) {
            try {
                Object cached = mekanism.common.config.MekanismConfig.general.maxUpgradeMultiplier;
                Class<?> type = cached.getClass();
                Field resolved = type.getDeclaredField("resolved");
                Field cachedValue = type.getDeclaredField("cachedValue");
                resolved.setAccessible(true);
                cachedValue.setAccessible(true);
                cachedValue.setInt(cached, value);
                resolved.setBoolean(cached, true);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        void begin(MekGameTestHelper helper, int speed, int energy) {
            begin(helper, speed, energy, 10);
        }

        /// `multiplier` é o `GeneralConfig.maxUpgradeMultiplier` (padrão 10), posto antes de a máquina nascer e devolvido em `write`.
        void begin(MekGameTestHelper helper, int speed, int energy, int multiplier) {
            this.helper = helper;
            if (multiplier != 10) {
                setMultiplier(multiplier);
                multiplierChanged = true;
            }
            Player player = helper.makeMockPlayerLookingAt(FLOOR, Direction.UP);
            helper.useOn(FLOOR, upgraded(speed, energy), player, Direction.UP);
        }

        private Holder<Upgrade> upgrade(ResourceKey<Upgrade> key) {
            return helper.getLevel().registryAccess().lookupOrThrow(MekanismRegistries.Keys.UPGRADES).getOrThrow(key);
        }

        private ItemStack upgraded(int speed, int energy) {
            ItemStack stack = new ItemStack(MekanismBlocks.METALLURGIC_INFUSER.asItem());
            Object2IntMap<Holder<Upgrade>> map = new Object2IntOpenHashMap<>();
            if (speed > 0) {
                map.put(upgrade(UpgradeIds.SPEED), speed);
            }
            if (energy > 0) {
                map.put(upgrade(UpgradeIds.ENERGY), energy);
            }
            if (!map.isEmpty()) {
                stack.set(MekanismDataComponents.UPGRADES, new UpgradeAware(map, LargeResourceStack.ITEM_HELPER.empty(), LargeResourceStack.ITEM_HELPER.empty()));
            }
            return stack;
        }

        TileEntityMetallurgicInfuser tile() {
            return (TileEntityMetallurgicInfuser) helper.getLevel().getBlockEntity(helper.absolutePos(SPOT));
        }

        MachineEnergyContainer<?> energy() {
            return (MachineEnergyContainer<?>) tile().getEnergyContainer();
        }

        /// Uma marca: o que o cenário fez depois daquele tick. As partes separadas por vírgula acontecem juntas.
        void apply(String mark) {
            marks.add(mark);
            for (String action : mark.split(", ")) {
                String[] words = action.split(" ");
                switch (words[0]) {
                    case "item" -> tile().getInventorySlots().get(INPUT).setContents(
                          Integer.parseInt(words[2]) == 0 ? ItemResource.EMPTY : ItemResource.of(item(words[1])), Integer.parseInt(words[2]), null);
                    case "tank" -> setTank(words[1], Long.parseLong(words[2]));
                    case "speed" -> setUpgrades(UpgradeIds.SPEED, Integer.parseInt(words[1]));
                    case "energy" -> setUpgrades(UpgradeIds.ENERGY, Integer.parseInt(words[1]));
                    case "reload" -> reload(false);
                    case "reload-without-usage" -> reload(true);
                    default -> throw new IllegalStateException(action);
                }
            }
        }

        private static Item item(String id) {
            return switch (id) {
                case "gravel" -> Items.GRAVEL;
                default -> throw new IllegalStateException(id);
            };
        }

        private void setTank(String chemical, long amount) {
            Holder<Chemical> holder = helper.getLevel().registryAccess().getOrThrow(chemical.equals("bio") ? ChemicalIds.BIO : ChemicalIds.REDSTONE);
            tile().infusionTank.setContents(amount == 0 ? ChemicalResource.EMPTY : ChemicalResource.of(holder), amount, null);
        }

        private void setUpgrades(ResourceKey<Upgrade> key, int total) {
            var component = tile().getComponent();
            Holder<Upgrade> upgrade = upgrade(key);
            int installed = component.getUpgrades(upgrade);
            if (total > installed) {
                component.addUpgrades(helper.getLevel().registryAccess(), upgrade, total - installed);
            }
            while (component.getUpgrades(upgrade) > total) {
                component.removeUpgrade(helper.getLevel().registryAccess(), upgrade, false);
            }
        }

        /// O que o jogo faz ao descarregar e carregar o chunk: grava o tile e cria outro do NBT (`BlockEntity.loadStatic`).
        private void reload(boolean withoutUsage) {
            TileEntityMetallurgicInfuser old = tile();
            BlockPos pos = helper.absolutePos(SPOT);
            CompoundTag tag = old.saveWithFullMetadata(helper.getLevel().registryAccess());
            if (withoutUsage) {
                //Um mundo gravado quando o tile ainda não guardava `used_so_far`: o `getIntOr` devolve 0.
                tag.remove("used_so_far");
            }
            BlockEntity loaded = BlockEntity.loadStatic(pos, helper.getLevel().getBlockState(pos), tag, helper.getLevel().registryAccess());
            if (!(loaded instanceof TileEntityMetallurgicInfuser) || loaded == old) {
                throw new IllegalStateException("loadStatic gave " + loaded);
            }
            LevelChunk chunk = helper.getLevel().getChunkAt(pos);
            old.onChunkUnloaded();
            chunk.removeBlockEntity(pos);
            chunk.addAndRegisterBlockEntity(loaded);
        }

        private static int usedSoFar(TileEntityMetallurgicInfuser infuser) {
            try {
                Field field = TileEntityMetallurgicInfuser.class.getDeclaredField("usedSoFar");
                field.setAccessible(true);
                return field.getInt(infuser);
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        private static void slot(StringBuilder line, net.minecraft.world.item.ItemStack stack) {
            line.append('"').append(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).getPath()).append("\",").append(stack.getCount());
        }

        void record() {
            if (helper == null || tile() == null) {
                return;
            }
            TileEntityMetallurgicInfuser infuser = tile();
            MachineEnergyContainer<?> energy = energy();
            StringBuilder line = new StringBuilder("{\"t\":").append(tick++).append(",\"marks\":[");
            for (int i = 0; i < marks.size(); i++) {
                line.append(i == 0 ? "" : ",").append('"').append(marks.get(i)).append('"');
            }
            marks.clear();
            line.append("],\"progress\":").append(infuser.getRecipeProgress())
                  .append(",\"required\":").append(infuser.getTicksRequired())
                  .append(",\"energy\":").append(energy.getAmountAsLong())
                  .append(",\"capacity\":").append(energy.getCapacityAsLong())
                  .append(",\"per_tick\":").append(energy.getEnergyPerTick())
                  .append(",\"active\":").append(infuser.getActive())
                  .append(",\"block_active\":").append(Attribute.isActive(infuser.getBlockState()))
                  .append(",\"powered\":").append(infuser.isPowered())
                  .append(",\"comparator\":").append(infuser.getCurrentRedstoneLevel())
                  .append(",\"input\":[");
            slot(line, infuser.getInventorySlots().get(INPUT).asItemStack());
            line.append("],\"output\":[");
            slot(line, infuser.getInventorySlots().get(OUTPUT).asItemStack());
            line.append("],\"errors\":[");
            boolean first = true;
            for (int i = 0; i < ERRORS.size(); i++) {
                if (infuser.getWarningCheck(ERRORS.get(i)).getAsBoolean()) {
                    line.append(first ? "" : ",").append('"').append(ERROR_NAMES.get(i)).append('"');
                    first = false;
                }
            }
            line.append("],\"tank\":").append(infuser.infusionTank.amountAsLong()).append(",\"infusion\":[");
            slot(line, infuser.getInventorySlots().get(INFUSION).asItemStack());
            line.append("],\"used_so_far\":").append(usedSoFar(infuser)).append(",\"max_operations\":").append(infuser.getOperationsPerTick()).append('}');
            lines.add(line.toString());
            energy.setEnergy(REFILL_TICKS * (long) energy.getEnergyPerTick(), null);
        }

        void write() {
            helper = null;
            if (multiplierChanged) {
                setMultiplier(10);
            }
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
