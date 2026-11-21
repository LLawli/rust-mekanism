package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import mekanism.api.RelativeSide;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.inventory.IInventorySlot;
import mekanism.api.resource.LargeResourceStack;
import mekanism.common.capabilities.Capabilities;
import mekanism.common.component.containers.type.ContainerType;
import mekanism.common.registries.MekanismDataComponents;
import net.minecraft.core.Holder;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.TooltipFlag;
import net.minecraft.world.level.GameType;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.api.MekanismRegistries;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.TileEntityChemicalTank;
import mekanism.common.tile.TileEntityFluidTank;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.tile.interfaces.ISideConfiguration;
import mekanism.common.util.ItemAccessUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.NbtOps;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
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

/// Frente `tanques`: grava o Fluid Tank e o Chemical Tank tick a tick (`tank_fluid_<cenário>.jsonl` e `tank_chemical_<cenário>.jsonl`).
///
/// Os tanques de fluido ficam numa coluna a partir de `(1,1,1)`, o índice 0 embaixo, sobre pedra; cada um é posto por `setBlock` e recebe o padrão do item
/// (`applyComponentsFromItemStack`). O tanque de químico fica em `(2,1,2)` com o padrão do item (a frente `OUTPUT`, o resto `INPUT`, ejetando) e os vizinhos são
/// tanques de químico com os seis lados `INPUT_OUTPUT` e sem ejetar (decisão 132 do port). As ações de cada tick rodam em `ServerTickEvent.Pre` e a linha sai em
/// `ServerTickEvent.Post`, depois do tick dos tiles. O resultado de cada `push` e `pull` (pela capability do bloco, numa transação raiz confirmada) sai em `results`,
/// na ordem das marcas; `break` grava o item solto pela tabela de loot e o NBT do tile, tira o bloco, e `place` o repõe com esse item.
@ForEachTest(groups = "trace")
public class TankTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final String COMMIT = "b2d04e166";
    private static final String DATE = "2026-11-21";
    private static final BlockPos FLUID_BASE = new BlockPos(1, 1, 1);
    private static final BlockPos CHEMICAL_POS = new BlockPos(2, 1, 2);
    private static final String[] TIERS = {"basic", "advanced", "elite", "ultimate", "creative"};

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(5, 6, 5).fill(0, 0, 0, 4, 0, 4, Blocks.STONE.defaultBlockState()));
    }

    private static int tierIndex(String tier) {
        for (int i = 0; i < TIERS.length; i++) {
            if (TIERS[i].equals(tier)) {
                return i;
            }
        }
        throw new IllegalArgumentException(tier);
    }

    private static Block fluidTankBlock(String tier) {
        return switch (tierIndex(tier)) {
            case 0 -> MekanismBlocks.BASIC_FLUID_TANK.value();
            case 1 -> MekanismBlocks.ADVANCED_FLUID_TANK.value();
            case 2 -> MekanismBlocks.ELITE_FLUID_TANK.value();
            case 3 -> MekanismBlocks.ULTIMATE_FLUID_TANK.value();
            default -> MekanismBlocks.CREATIVE_FLUID_TANK.value();
        };
    }

    private static Block chemicalTankBlock(String tier) {
        return switch (tierIndex(tier)) {
            case 0 -> MekanismBlocks.BASIC_CHEMICAL_TANK.value();
            case 1 -> MekanismBlocks.ADVANCED_CHEMICAL_TANK.value();
            case 2 -> MekanismBlocks.ELITE_CHEMICAL_TANK.value();
            case 3 -> MekanismBlocks.ULTIMATE_CHEMICAL_TANK.value();
            default -> MekanismBlocks.CREATIVE_CHEMICAL_TANK.value();
        };
    }

    private static Fluid fluidOf(String fluid) {
        return switch (fluid) {
            case "water" -> Fluids.WATER;
            case "lava" -> Fluids.LAVA;
            default -> throw new IllegalArgumentException(fluid);
        };
    }

    private static ChemicalResource chemicalOf(String chemical) {
        ResourceKey<Chemical> key = ResourceKey.create(MekanismRegistries.Keys.CHEMICAL, Identifier.fromNamespaceAndPath("mekanism", chemical));
        return ChemicalResource.of(ServerLifecycleHooks.getCurrentServer().registryAccess().getOrThrow(key));
    }

    private static Direction sideOf(String name) {
        return name.equals("null") ? null : Direction.byName(name);
    }

    /// `item` ou `item:quantidade`, ou `ftank:<tier>:<fluido>:<mB>:<quantidade>` (o item do Fluid Tank com fluido) e
    /// `ctank:<tier>:<químico>:<mB>:<quantidade>` (o do Chemical Tank), cheios pela capability do item até `mB` ou até ela parar de aceitar.
    private static ItemStack stackOf(String spec) {
        String[] part = spec.split(":");
        switch (part[0]) {
            case "ftank" -> {
                Item item = fluidTankBlock(part[1]).asItem();
                ItemAccess access = ItemAccessUtils.sideEffectFreeAccess(ItemResource.of(item));
                long wanted = Long.parseLong(part[3]);
                if (wanted > 0) {
                    FluidResource resource = FluidResource.of(fluidOf(part[2]));
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
                stack.setCount(Integer.parseInt(part[4]));
                return stack;
            }
            case "ctank" -> {
                Item item = chemicalTankBlock(part[1]).asItem();
                ItemAccess access = ItemAccessUtils.sideEffectFreeAccess(ItemResource.of(item));
                long wanted = Long.parseLong(part[3]);
                if (wanted > 0) {
                    ChemicalResource resource = chemicalOf(part[2]);
                    long stored = 0;
                    for (int i = 0; i < 100_000 && stored < wanted; i++) {
                        ResourceHandler<ChemicalResource> handler = Capabilities.CHEMICAL.getCapability(access);
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
                stack.setCount(Integer.parseInt(part[4]));
                return stack;
            }
            case "ftankfull" -> {
                // `ftankfull:<tier>:<fluido>:<quantidade>`: o item cheio como a aba criativa o faz (`getFilledVariant`, direto no contêiner).
                Holder<Item> holder = BuiltInRegistries.ITEM.wrapAsHolder(fluidTankBlock(part[1]).asItem());
                ItemStack stack = ContainerType.FLUID.getFilledVariant(holder, FluidResource.of(fluidOf(part[2])), null);
                stack.setCount(Integer.parseInt(part[3]));
                return stack;
            }
            case "ctankfull" -> {
                Holder<Item> holder = BuiltInRegistries.ITEM.wrapAsHolder(chemicalTankBlock(part[1]).asItem());
                ItemStack stack = ContainerType.CHEMICAL.getFilledVariant(holder, chemicalOf(part[2]), null);
                stack.setCount(Integer.parseInt(part[3]));
                return stack;
            }
            default -> {
                Item item = switch (part[0]) {
                    case "water_bucket" -> Items.WATER_BUCKET;
                    case "lava_bucket" -> Items.LAVA_BUCKET;
                    case "bucket" -> Items.BUCKET;
                    case "stick" -> Items.STICK;
                    case "none" -> Items.AIR;
                    default -> throw new IllegalArgumentException(spec);
                };
                return item == Items.AIR ? ItemStack.EMPTY : new ItemStack(item, part.length > 1 ? Integer.parseInt(part[1]) : 1);
            }
        }
    }

    /// `lightUpdateDelay`, que é privado.
    private static int lightDelay(TileEntityFluidTank tile) {
        try {
            java.lang.reflect.Field field = TileEntityFluidTank.class.getDeclaredField("lightUpdateDelay");
            field.setAccessible(true);
            return field.getInt(tile);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    private static void setSlot(IInventorySlot slot, ItemStack stack) {
        if (stack.isEmpty()) {
            slot.setContents(LargeResourceStack.ITEM_HELPER.empty(), null);
        } else {
            slot.setContents(ItemResource.of(stack), stack.getCount(), null);
        }
    }

    /// `[item, quantidade, fluido do item, mB, químico do item, mB]`: o conteúdo é o do primeiro contêiner da capability do item, lido numa cópia de um só.
    private static JsonArray slotJson(IInventorySlot slot) {
        return stackJson(slot.asItemStack());
    }

    private static JsonArray stackJson(ItemStack stack) {
        JsonArray array = new JsonArray();
        array.add(stack.isEmpty() ? "" : BuiltInRegistries.ITEM.getKey(stack.getItem()).toString());
        array.add(stack.getCount());
        String fluid = "";
        long fluidAmount = 0;
        String chemical = "";
        long chemicalAmount = 0;
        if (!stack.isEmpty()) {
            ItemAccess access = ItemAccessUtils.sideEffectFreeAccess(ItemResource.of(stack));
            ResourceHandler<FluidResource> fluids = Capabilities.FLUID.getCapability(access);
            if (fluids != null && fluids.size() > 0 && !fluids.getResource(0).isEmpty()) {
                fluid = fluids.getResource(0).typeHolder().getRegisteredName();
                fluidAmount = fluids.getAmountAsLong(0);
            }
            ResourceHandler<ChemicalResource> chemicals = Capabilities.CHEMICAL.getCapability(access);
            if (chemicals != null && chemicals.size() > 0 && !chemicals.getResource(0).isEmpty()) {
                chemical = chemicals.getResource(0).typeHolder().getRegisteredName();
                chemicalAmount = chemicals.getAmountAsLong(0);
            }
        }
        array.add(fluid);
        array.add(fluidAmount);
        array.add(chemical);
        array.add(chemicalAmount);
        return array;
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Cenários de fluido
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The input and output slots of a basic fluid tank in the default BOTH mode.")
    public static void fluidSlotsBoth(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("fluid", "slots_both", "One basic fluid tank (32000 mB, 1000 mB per tick), edit mode BOTH. A water bucket at 0, an empty bucket at 10, 16 empty buckets at 20 with 9000 of water, a lava bucket at 45 "
              + "(wrong fluid: moved out), a stick at 55, a water bucket at 60 with the output slot holding 16 buckets (full), the output freed at 70, a water bucket at 80 with the tank full.");
        scene.fluidTanks("basic");
        scene.at(0, "in 0 water_bucket").at(10, "in 0 bucket").at(20, "out 0 none").at(20, "fluid 0 water 9000").at(20, "in 0 bucket:16").at(44, "out 0 none").at(45, "in 0 lava_bucket")
              .at(54, "out 0 none").at(55, "in 0 stick").at(59, "in 0 none").at(59, "out 0 bucket:16").at(60, "in 0 water_bucket").at(70, "out 0 none").at(79, "out 0 none")
              .at(79, "fluid 0 water 32000").at(80, "in 0 water_bucket");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The edit modes FILL and EMPTY of a fluid tank, and what the mode change does to a stack in the slot.")
    public static void fluidSlotsModes(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("fluid", "slots_modes", "One advanced fluid tank (64000 mB, 4000 mB per tick). Mode FILL at 0 (nextMode): a water bucket at 1 (not emptied), 5000 of water and an empty bucket at 10 (filled). "
              + "Mode EMPTY at 20: an empty bucket at 21 (stays), a water bucket at 30 (emptied), a lava bucket at 40 (stays in the input slot). Back to BOTH at 50 (nextMode) with the lava bucket still there; "
              + "previousMode at 60 (EMPTY), 4 empty buckets at 61, previousMode at 70 (FILL).");
        scene.fluidTanks("advanced");
        scene.at(0, "mode 0 next").at(1, "in 0 water_bucket").at(10, "in 0 none").at(10, "fluid 0 water 5000").at(10, "in 0 bucket").at(20, "mode 0 next").at(21, "out 0 none").at(21, "in 0 bucket")
              .at(30, "in 0 water_bucket").at(40, "out 0 none").at(40, "in 0 lava_bucket").at(50, "mode 0 next").at(60, "mode 0 prev").at(61, "out 0 none").at(61, "in 0 bucket:4").at(70, "mode 0 prev");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "Fluid tank items in the slots of a fluid tank: the rate limit of the tank and of the item.")
    public static void fluidSlotsTankItem(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("fluid", "slots_tank_item", "One elite fluid tank (128000 mB, 16000 mB per tick). A basic fluid tank item with 32000 of water at 0, an empty basic tank item at 20, an ultimate tank item with 100000 of lava at 45 "
              + "(wrong fluid), the tank emptied at 60 with the lava item put back, 2 empty basic tank items (a stack) at 80 with 50000 of lava.");
        scene.fluidTanks("elite");
        scene.at(0, "in 0 ftank:basic:water:32000:1").at(20, "out 0 none").at(20, "in 0 ftank:basic:water:0:1").at(45, "out 0 none").at(45, "in 0 ftank:ultimate:lava:100000:1").at(60, "out 0 none")
              .at(60, "fluid 0 none 0").at(60, "in 0 ftank:ultimate:lava:100000:1").at(80, "out 0 none").at(80, "in 0 none").at(80, "fluid 0 lava 50000").at(80, "in 0 ftank:basic:water:0:2");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "A column of three fluid tanks: the push down of an active tank, the overflow to the tank above and the valve.")
    public static void fluidColumn(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("fluid", "column", "Three fluid tanks in a column: basic (0, bottom), advanced (1) and basic (2, top). 20000 of water in the top one at 0; the top one goes active at 5 and the middle one at 20; "
              + "the middle one goes inactive at 40. At 50, 40000 of water pushed into the top face of the bottom tank (overflows up, the valve of the bottom one), at 52 500 more, at 60 100000 pushed into the north face of the "
              + "bottom tank, at 70 lava pushed into the top of the bottom one (refused), at 75 3000 pulled from the bottom face of the bottom tank and at 80 from the south face of the top one. The top tank goes active at 85.");
        scene.fluidTanks("basic", "advanced", "basic");
        scene.at(0, "fluid 2 water 20000").at(5, "active 2 true").at(20, "active 1 true").at(40, "active 1 false").at(50, "push 0 up water 40000").at(52, "push 0 up water 500").at(60, "push 0 north water 100000")
              .at(70, "push 0 up lava 1000").at(75, "pull 0 down water 3000").at(80, "pull 2 south water 3000").at(85, "active 2 true");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "A column of the four tiers, all active, draining into the bottom one, with the scale and the comparator of each.")
    public static void fluidTiers(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("fluid", "tiers", "Four fluid tanks in a column: ultimate (0, bottom), elite (1), advanced (2) and basic (3, top), the three upper ones active from 0. The top three are filled with lava at 0 "
              + "(32000, 64000 and 128000); the bottom one is emptied at 60 and filled to 255000 at 70.");
        scene.fluidTanks("ultimate", "elite", "advanced", "basic");
        scene.at(0, "fluid 3 lava 32000").at(0, "fluid 2 lava 64000").at(0, "fluid 1 lava 128000").at(0, "active 3 true").at(0, "active 2 true").at(0, "active 1 true").at(60, "fluid 0 none 0")
              .at(70, "fluid 0 lava 255000");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The creative fluid tank: the slots, the push from outside and the tank below it.")
    public static void fluidCreative(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("fluid", "creative", "A basic fluid tank (0, bottom) under a creative one (1). A water bucket in the creative one at 0 (sets the type, the bucket is not emptied), an empty bucket at 10, lava pushed into its north "
              + "face at 20 and water at 22 (accepted, nothing changes), 5000 pulled at 25. Active at 30 (fills the one below). Emptied at 50 and 7000 of lava pushed in at 52 (an empty creative tank voids), "
              + "a lava bucket at 60, mode EMPTY at 70 (two nextMode) and an empty bucket at 71.");
        scene.fluidTanks("basic", "creative");
        scene.at(0, "in 1 water_bucket").at(10, "out 1 none").at(10, "in 1 bucket").at(20, "push 1 north lava 5000").at(22, "push 1 north water 5000").at(25, "pull 1 north water 5000").at(30, "active 1 true")
              .at(49, "active 1 false").at(50, "fluid 1 none 0").at(52, "push 1 north lava 7000").at(60, "out 1 none").at(60, "in 1 lava_bucket").at(70, "mode 1 next").at(70, "mode 1 next")
              .at(71, "out 1 none").at(71, "in 1 bucket");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "A fluid tank broken and placed back: the item of the loot table and the tile it makes.")
    public static void fluidItem(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("fluid", "item", "One basic fluid tank with 12345 of water, active, mode FILL and an empty bucket stack of 3 in the input slot from 0. Broken at 20 (the drop of the loot table and the NBT) and placed back "
              + "with the dropped item at 22. Emptied at 40, broken at 50 and placed back at 52.");
        scene.fluidTanks("basic");
        scene.at(0, "fluid 0 water 12345").at(0, "active 0 true").at(0, "mode 0 next").at(0, "in 0 bucket:3").at(20, "break 0").at(22, "place 0").at(40, "fluid 0 none 0").at(40, "in 0 none").at(40, "out 0 none")
              .at(50, "break 0").at(52, "place 0");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The edit mode change clears the last transfer direction of the input slot.")
    public static void fluidModeReset(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("fluid", "mode_reset", "One basic fluid tank in mode FILL from 0 (nextMode). 500 of water and an empty basic tank item in the input slot at 1 (the 500 go into the "
              + "item, the direction is DRAIN_INTO_ITEM and the item stays). previousMode at 10 (BOTH): with the direction cleared the tank tries to drain, cannot, and the item is emptied "
              + "back into the tank and then moved to the output slot; with DRAIN_INTO_ITEM kept the item would stay with its 500.");
        scene.fluidTanks("basic");
        scene.at(0, "mode 0 next").at(1, "fluid 0 water 500").at(1, "in 0 ftank:basic:water:0:1").at(10, "mode 0 previous");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The light update delay of a fluid tank that fills and empties.")
    public static void fluidLight(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("fluid", "light", "One basic fluid tank. 5000 of lava at 5 (the delay goes to 1), emptied at 20 (the delay goes to blockDeactivationDelay, 60), 100 of lava at 30 and "
              + "emptied at 40 (the delay is running: it is not set again), 8000 of water at 90.");
        scene.fluidTanks("basic");
        scene.at(5, "fluid 0 lava 5000").at(20, "fluid 0 none 0").at(30, "fluid 0 lava 100").at(40, "fluid 0 none 0").at(90, "fluid 0 water 8000");
        scene.run(test);
    }

    /// O item do Fluid Tank usado no mundo (`ItemBlockFluidTank.use`) e o tooltip dos itens dos dois tanques. Tudo num tick só: cada passo monta o bloco alvo em `(2,1,2)`, sobre a
    /// pedra de `(2,0,2)`, põe a pilha na mão de um jogador de teste que olha para baixo, de `(2.5, 3.2, 2.5)`, e chama `use`.
    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The fluid tank item used in the world in bucket mode, and the tooltips of the tank items.")
    public static void fluidItemUse(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence().thenExecute(() -> {
            List<String> lines = new ArrayList<>();
            RegistryOps<JsonElement> json = RegistryOps.create(JsonOps.INSTANCE, helper.getLevel().registryAccess());
            JsonObject meta = new JsonObject();
            meta.addProperty("commit", COMMIT);
            meta.addProperty("date", DATE);
            meta.addProperty("file", "tank_fluid_item_use.jsonl");
            meta.addProperty("scenario", "ItemBlockFluidTank.use, one step per line, all in one tick. step: name; block: what is at the target before (air, water, lava: a source); stack: the spec of the "
                  + "stack in the main hand; mode: mekanism:bucket_mode; sneak; creative: the game mode of the player. first: the result of a first use, which in bucket mode only claims the item (tryClaimItem); result: the one of the second use, success, fail or pass; hand and inventory (the other "
                  + "non-empty slots, [slot, ...]) as [item, count, fluid of the item, mB, chemical, mB]; after: the block and the fluid state at the target. Then one line per tooltip: "
                  + "{tooltip: spec, mode, lines: the components of getTooltipLines(NORMAL) as ComponentSerialization.CODEC writes them}.");
            JsonObject root = new JsonObject();
            root.add("meta", meta);
            lines.add(root.toString());
            // nome, bloco, pilha, modo balde, agachado, jogador criativo
            String[][] steps = {
                  {"mode_off_water", "water", "ftank:basic:water:0:1", "false", "false", "false"},
                  {"pickup_water", "water", "ftank:basic:water:0:1", "true", "false", "false"},
                  {"pickup_lava_into_water", "lava", "ftank:basic:water:5000:1", "true", "false", "false"},
                  {"pickup_nearly_full", "water", "ftank:basic:water:31500:1", "true", "false", "false"},
                  {"pickup_full", "water", "ftank:basic:water:32000:1", "true", "false", "false"},
                  {"pickup_air", "air", "ftank:basic:water:0:1", "true", "false", "false"},
                  {"pickup_stack_of_2", "water", "ftank:basic:water:0:2", "true", "false", "false"},
                  {"pickup_creative_item", "water", "ftank:creative:water:0:1", "true", "false", "false"},
                  {"pickup_creative_player", "water", "ftank:basic:water:0:1", "true", "false", "true"},
                  {"place_water", "air", "ftank:basic:water:5000:1", "true", "true", "false"},
                  {"place_lava", "air", "ftank:advanced:lava:2000:1", "true", "true", "false"},
                  {"place_empty", "air", "ftank:basic:water:0:1", "true", "true", "false"},
                  {"place_partial", "air", "ftank:basic:water:500:1", "true", "true", "false"},
                  {"place_into_water", "water", "ftank:basic:water:5000:1", "true", "true", "false"},
                  {"place_water_into_lava", "lava", "ftank:basic:water:5000:1", "true", "true", "false"},
                  {"place_stack_of_2", "air", "ftank:basic:water:3000:2", "true", "true", "false"},
                  {"place_creative_item", "air", "ftankfull:creative:water:1", "true", "true", "false"},
                  {"place_creative_player", "air", "ftank:basic:water:5000:1", "true", "true", "true"},
                  {"place_mode_off", "air", "ftank:basic:water:5000:1", "false", "true", "false"},
            };
            BlockPos target = new BlockPos(2, 1, 2);
            BlockPos absolute = helper.absolutePos(target);
            ServerLevel level = helper.getLevel();
            Player survival = helper.makeTickingMockServerPlayerInLevel(GameType.SURVIVAL);
            Player creative = helper.makeTickingMockServerPlayerInLevel(GameType.CREATIVE);
            for (String[] step : steps) {
                Player player = Boolean.parseBoolean(step[5]) ? creative : survival;
                player.getInventory().clearContent();
                player.snapTo(absolute.getX() + 0.5, absolute.getY() + 2.2 - player.getEyeHeight(), absolute.getZ() + 0.5, 0, 90);
                player.setShiftKeyDown(Boolean.parseBoolean(step[4]));
                helper.setBlock(target, switch (step[1]) {
                    case "water" -> Blocks.WATER.defaultBlockState();
                    case "lava" -> Blocks.LAVA.defaultBlockState();
                    default -> Blocks.AIR.defaultBlockState();
                });
                ItemStack stack = stackOf(step[2]);
                stack.set(MekanismDataComponents.BUCKET_MODE, Boolean.parseBoolean(step[3]));
                player.setItemInHand(InteractionHand.MAIN_HAND, stack);
                // O primeiro uso no modo balde só toma posse do item (`tryClaimItem`); o segundo é o que age.
                InteractionResult first = stack.use(level, player, InteractionHand.MAIN_HAND);
                InteractionResult result = player.getItemInHand(InteractionHand.MAIN_HAND).use(level, player, InteractionHand.MAIN_HAND);
                JsonObject line = new JsonObject();
                line.addProperty("first", first instanceof InteractionResult.Success ? "success" : first instanceof InteractionResult.Fail ? "fail"
                                                                                                 : first instanceof InteractionResult.Pass ? "pass" : first.toString());
                line.addProperty("step", step[0]);
                line.addProperty("block", step[1]);
                line.addProperty("stack", step[2]);
                line.addProperty("mode", Boolean.parseBoolean(step[3]));
                line.addProperty("sneak", Boolean.parseBoolean(step[4]));
                line.addProperty("creative", Boolean.parseBoolean(step[5]));
                line.addProperty("result", result instanceof InteractionResult.Success ? "success" : result instanceof InteractionResult.Fail ? "fail"
                                                                                                    : result instanceof InteractionResult.Pass ? "pass" : result.toString());
                if (result instanceof InteractionResult.Success success && success.heldItemTransformedTo() != null) {
                    line.add("transformed", stackJson(success.heldItemTransformedTo()));
                }
                int selected = player.getInventory().getSelectedSlot();
                line.add("hand", stackJson(player.getInventory().getItem(selected)));
                JsonArray inventory = new JsonArray();
                for (int slot = 0; slot < player.getInventory().getContainerSize(); slot++) {
                    ItemStack other = player.getInventory().getItem(slot);
                    if (slot != selected && !other.isEmpty()) {
                        JsonArray entry = stackJson(other);
                        JsonArray withSlot = new JsonArray();
                        withSlot.add(slot);
                        withSlot.addAll(entry);
                        inventory.add(withSlot);
                    }
                }
                line.add("inventory", inventory);
                BlockState after = level.getBlockState(absolute);
                JsonArray afterJson = new JsonArray();
                afterJson.add(BuiltInRegistries.BLOCK.getKey(after.getBlock()).toString());
                afterJson.add(after.getFluidState().isEmpty() ? "" : BuiltInRegistries.FLUID.getKey(after.getFluidState().getType()).toString());
                afterJson.add(after.getFluidState().isSource());
                line.add("after", afterJson);
                lines.add(line.toString());
            }
            helper.setBlock(target, Blocks.AIR.defaultBlockState());
            survival.setShiftKeyDown(false);
            // especificação da pilha, modo balde
            String[][] tips = {
                  {"ftank:basic:water:0:1", "false"}, {"ftank:basic:water:5000:1", "false"}, {"ftank:basic:water:5000:1", "true"}, {"ftank:ultimate:lava:256000:1", "false"},
                  {"ftank:creative:water:0:1", "false"}, {"ftankfull:creative:water:1", "false"},
                  {"ctank:basic:hydrogen:0:1", ""}, {"ctank:basic:hydrogen:5000:1", ""}, {"ctank:ultimate:oxygen:8192000:1", ""}, {"ctank:creative:hydrogen:0:1", ""},
                  {"ctankfull:creative:hydrogen:1", ""},
            };
            Item.TooltipContext context = Item.TooltipContext.of(level.registryAccess());
            for (String[] tip : tips) {
                ItemStack stack = stackOf(tip[0]);
                if (!tip[1].isEmpty()) {
                    stack.set(MekanismDataComponents.BUCKET_MODE, Boolean.parseBoolean(tip[1]));
                }
                JsonObject line = new JsonObject();
                line.addProperty("tooltip", tip[0]);
                line.addProperty("mode", tip[1]);
                JsonArray out = new JsonArray();
                for (Component text : stack.getTooltipLines(context, survival, TooltipFlag.Default.NORMAL)) {
                    out.add(ComponentSerialization.CODEC.encodeStart(json, text).getOrThrow());
                }
                line.add("lines", out);
                lines.add(line.toString());
            }
            try {
                Path file = Path.of("trace", "tank_fluid_item_use.jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }).thenSucceed());
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------
    // Cenários de químico
    // ------------------------------------------------------------------------------------------------------------------------------------------------

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The dumping modes of the four tiers of chemical tank.")
    public static void chemicalDump(final DynamicTest test) {
        dump(test, "basic");
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The dumping modes of the advanced chemical tank.")
    public static void chemicalDumpAdvanced(final DynamicTest test) {
        dump(test, "advanced");
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The dumping modes of the elite chemical tank.")
    public static void chemicalDumpElite(final DynamicTest test) {
        dump(test, "elite");
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The dumping modes of the ultimate chemical tank.")
    public static void chemicalDumpUltimate(final DynamicTest test) {
        dump(test, "ultimate");
    }

    private static void dump(DynamicTest test, String tier) {
        template(test);
        Scene scene = new Scene("chemical", "dump_" + tier, "One " + tier + " chemical tank, full of hydrogen, with no neighbour and ejecting off. DUMPING_EXCESS at 10 (nextMode), DUMPING at 40, IDLE at 70; refilled at 75 and "
              + "DUMPING_EXCESS again at 80; set to 1 unit over 90 % of the capacity at 90.");
        scene.chemicalTank(tier);
        scene.at(0, "eject false").at(0, "chem hydrogen -1").at(10, "dump").at(40, "dump").at(70, "dump").at(75, "chem hydrogen -1").at(80, "dump").at(90, "chem hydrogen -90");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The ejector of a chemical tank with two real receivers, and the dumping mode that stops it with room in the receivers.")
    public static void chemicalEjectTwo(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("chemical", "eject_two", "An advanced chemical tank (256000 mB, 16000 per tick) facing north with the item default (front OUTPUT, ejecting on), 200000 of hydrogen. "
              + "Receivers: a basic chemical tank north (the front) and one east (the LEFT side of a tank facing north). The left side goes OUTPUT at 10, ejecting off at 20 with the two "
              + "receivers emptied, on at 30 (the split between two), DUMPING_EXCESS at 40 and DUMPING at 50, both with the receivers emptied and the tank back to 200000 (in DUMPING the "
              + "ejector stops with room in the receivers), IDLE at 60. A creative tank item full of hydrogen in the fill slot at 80, with the tank and the receivers emptied and ejecting off.");
        scene.chemicalTank("advanced").receiver(Direction.NORTH, "basic").receiver(Direction.EAST, "basic");
        scene.at(0, "chem hydrogen 200000").at(10, "side left output").at(20, "eject false").at(20, "recv north none 0").at(20, "recv east none 0").at(30, "eject true")
              .at(40, "recv north none 0").at(40, "recv east none 0").at(40, "chem hydrogen 200000").at(40, "dump")
              .at(50, "recv north none 0").at(50, "recv east none 0").at(50, "chem hydrogen 200000").at(50, "dump").at(60, "dump")
              .at(80, "eject false").at(80, "recv north none 0").at(80, "recv east none 0").at(80, "chem none 0").at(80, "fillslot ctankfull:creative:hydrogen:1");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The ejector of a chemical tank: the front, a second side, the dumping mode that stops it.")
    public static void chemicalEject(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("chemical", "eject", "An advanced chemical tank (256000 mB, 16000 per tick) with the item default (front, north, OUTPUT; ejecting on), 100000 of hydrogen. Receivers: a basic chemical tank north and one west. "
              + "The left side (west) goes OUTPUT at 10 (two receivers), ejecting off at 20 and on at 30, DUMPING_EXCESS at 40, DUMPING at 50 (the ejector stops), IDLE at 60. The north receiver gets oxygen at 70 and the tank 90000 of hydrogen.");
        scene.chemicalTank("advanced").receiver(Direction.NORTH, "basic").receiver(Direction.WEST, "basic");
        scene.at(0, "chem hydrogen 100000").at(10, "side left output").at(20, "eject false").at(30, "eject true").at(40, "dump").at(50, "dump").at(60, "dump").at(70, "recv north oxygen 5000")
              .at(70, "recv west none 0").at(70, "chem hydrogen 90000");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The two slots of a chemical tank with chemical tank items.")
    public static void chemicalSlots(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("chemical", "slots", "A basic chemical tank (64000 mB, 1000 per tick), ejecting off. Fill slot (index 1): a basic tank item with 5000 of hydrogen at 0, an advanced one with 100000 of oxygen at 15 (wrong type), "
              + "a stick at 25. Drain slot (index 0): an empty basic tank item at 30, a stack of 2 empty basic tank items at 50, an elite tank item with 1000 of oxygen at 60 (wrong type), a creative tank item at 70. The tank is refilled at 45.");
        scene.chemicalTank("basic");
        scene.at(0, "eject false").at(0, "fillslot ctank:basic:hydrogen:5000:1").at(15, "fillslot ctank:advanced:oxygen:100000:1").at(25, "fillslot stick").at(30, "drainslot ctank:basic:hydrogen:0:1")
              .at(45, "chem hydrogen 30000").at(50, "drainslot ctank:basic:hydrogen:0:2").at(60, "drainslot ctank:elite:oxygen:1000:1").at(70, "drainslot ctank:creative:hydrogen:0:1");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "The creative chemical tank: ejecting without losing, the slots and the push from outside.")
    public static void chemicalCreative(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("chemical", "creative", "A creative chemical tank with the item default and a basic receiver north. Hydrogen pushed into the west face at 0 (an empty creative tank voids), a basic tank item with 3000 of hydrogen "
              + "in the fill slot at 5 (sets the type), the receiver emptied at 30, DUMPING at 40 (the creative keeps ejecting and does not dump), oxygen pushed at 50 and hydrogen at 52, 5000 pulled from the north face at 55, "
              + "an empty basic tank item in the drain slot at 60.");
        scene.chemicalTank("creative").receiver(Direction.NORTH, "basic");
        scene.at(0, "push west hydrogen 5000").at(5, "fillslot ctank:basic:hydrogen:3000:1").at(30, "recv north none 0").at(40, "dump").at(40, "dump").at(50, "push west oxygen 5000").at(52, "push west hydrogen 5000")
              .at(55, "pull north hydrogen 5000").at(60, "drainslot ctank:basic:hydrogen:0:1");
        scene.run(test);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_tank")
    @TestHolder(description = "A chemical tank broken and placed back: the item of the loot table and the tile it makes, and the push and pull by side.")
    public static void chemicalItem(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("chemical", "item", "A basic chemical tank with the item default, ejecting off, 23456 of oxygen, DUMPING_EXCESS, the left side OUTPUT. Pushes and pulls of 100 on the six faces from 2 to 13. Broken at 20 and placed "
              + "back with the dropped item at 22; emptied at 40, broken at 50 and placed back at 52.");
        scene.chemicalTank("basic");
        scene.at(0, "eject false").at(0, "chem oxygen 23456").at(0, "dump").at(0, "side left output");
        int tick = 2;
        for (Direction side : Direction.values()) {
            scene.at(tick++, "push " + side.getSerializedName() + " oxygen 100");
            scene.at(tick++, "pull " + side.getSerializedName() + " oxygen 100");
        }
        scene.at(20, "break").at(22, "place").at(40, "chem none 0").at(50, "break").at(52, "place");
        scene.run(test);
    }

    // ------------------------------------------------------------------------------------------------------------------------------------------------

    private static final class Scene {

        static final int TICKS = 100;

        final String kind;
        final String name;
        final String description;
        final Map<Integer, List<String>> script = new HashMap<>();
        final List<String> fluidTiers = new ArrayList<>();
        String chemicalTier;
        final List<Direction> receiverFaces = new ArrayList<>();
        final List<String> receiverTiers = new ArrayList<>();
        final List<String> lines = new ArrayList<>();
        final List<String> marks = new ArrayList<>();
        final JsonArray results = new JsonArray();
        final JsonArray drops = new JsonArray();
        final Map<Integer, ItemStack> held = new HashMap<>();
        MekGameTestHelper helper;
        RegistryOps<JsonElement> json;
        boolean armed;
        boolean running;
        boolean done;
        int tick;

        Scene(String kind, String name, String description) {
            this.kind = kind;
            this.name = name;
            this.description = description;
        }

        Scene fluidTanks(String... tiers) {
            fluidTiers.addAll(List.of(tiers));
            return this;
        }

        Scene chemicalTank(String tier) {
            chemicalTier = tier;
            return this;
        }

        Scene receiver(Direction face, String tier) {
            receiverFaces.add(face);
            receiverTiers.add(tier);
            return this;
        }

        Scene at(int tick, String action) {
            script.computeIfAbsent(tick, t -> new ArrayList<>()).add(action);
            return this;
        }

        void run(DynamicTest test) {
            test.eventListeners().forge().addListener((final ServerTickEvent.Pre event) -> pre());
            test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> post());
            test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
                  .thenExecute(() -> begin(helper))
                  .thenExecuteAfter(TICKS + 20, this::finish)
                  .thenSucceed());
        }

        ServerLevel level() {
            return helper.getLevel();
        }

        BlockPos fluidPos(int index) {
            return helper.absolutePos(FLUID_BASE.above(index));
        }

        TileEntityFluidTank fluidTank(int index) {
            return level().getBlockEntity(fluidPos(index)) instanceof TileEntityFluidTank tank ? tank : null;
        }

        TileEntityChemicalTank chemicalTank() {
            return level().getBlockEntity(helper.absolutePos(CHEMICAL_POS)) instanceof TileEntityChemicalTank tank ? tank : null;
        }

        TileEntityChemicalTank receiver(Direction face) {
            return (TileEntityChemicalTank) level().getBlockEntity(helper.absolutePos(CHEMICAL_POS.relative(face)));
        }

        void placeFluid(int index, ItemStack item) {
            helper.setBlock(FLUID_BASE.above(index), fluidTankBlock(fluidTiers.get(index)).defaultBlockState());
            fluidTank(index).applyComponentsFromItemStack(item);
        }

        void placeChemical(ItemStack item) {
            helper.setBlock(CHEMICAL_POS, chemicalTankBlock(chemicalTier).defaultBlockState());
            chemicalTank().applyComponentsFromItemStack(item);
        }

        void setChemical(TileEntityChemicalTank tile, String chemical, long amount) {
            if (chemical.equals("none")) {
                tile.getChemicalTank().setContents(LargeResourceStack.CHEMICAL_HELPER.empty(), null);
                return;
            }
            ChemicalResource resource = chemicalOf(chemical);
            long capacity = tile.getChemicalTank().capacityAsLong(resource);
            // -1 is full; -90 is one unit over 90 % of the capacity (the default dumpExcessKeepRatio)
            long value = amount == -1 ? capacity : amount == -90 ? capacity / 10 * 9 + 1 : amount;
            tile.getChemicalTank().setContents(resource, value, null);
        }

        void breakTile(int index, TileEntityMekanism tile, BlockPos pos) {
            BlockState state = level().getBlockState(pos);
            JsonObject entry = new JsonObject();
            entry.addProperty("t", tick);
            entry.add("nbt", NbtOps.INSTANCE.convertTo(JsonOps.INSTANCE, tile.saveWithFullMetadata(level().registryAccess())));
            List<ItemStack> dropped = Block.getDrops(state, level(), pos, tile);
            JsonArray array = new JsonArray();
            for (ItemStack drop : dropped) {
                array.add(ItemStack.CODEC.encodeStart(json, drop).getOrThrow());
            }
            entry.add("stacks", array);
            drops.add(entry);
            held.put(index, dropped.isEmpty() ? ItemStack.EMPTY : dropped.get(0).copy());
            level().removeBlockEntity(pos);
            level().setBlockAndUpdate(pos, Blocks.AIR.defaultBlockState());
        }

        void apply(String action) {
            String[] part = action.split(" ");
            if (kind.equals("fluid")) {
                int index = Integer.parseInt(part[1]);
                TileEntityFluidTank tile = fluidTank(index);
                switch (part[0]) {
                    case "fluid" -> {
                        if (part[2].equals("none")) {
                            tile.fluidTank.setContents(LargeResourceStack.FLUID_HELPER.empty(), null);
                        } else {
                            tile.fluidTank.setContents(FluidResource.of(fluidOf(part[2])), Long.parseLong(part[3]), null);
                        }
                    }
                    case "in" -> setSlot(tile.getInventorySlots().get(0), stackOf(part[2]));
                    case "out" -> setSlot(tile.getInventorySlots().get(1), stackOf(part[2]));
                    case "mode" -> {
                        if (part[2].equals("next")) {
                            tile.nextMode();
                        } else {
                            tile.previousMode();
                        }
                    }
                    case "active" -> tile.setActive(Boolean.parseBoolean(part[2]));
                    case "push", "pull" -> {
                        ResourceHandler<FluidResource> handler = level().getCapability(Capabilities.FLUID.block(), fluidPos(index), sideOf(part[2]));
                        int moved = -1;
                        if (handler != null) {
                            FluidResource resource = FluidResource.of(fluidOf(part[3]));
                            try (Transaction transaction = Transaction.openRoot()) {
                                moved = part[0].equals("push") ? handler.insert(resource, Integer.parseInt(part[4]), transaction) : handler.extract(resource, Integer.parseInt(part[4]), transaction);
                                transaction.commit();
                            }
                        }
                        results.add(moved);
                    }
                    case "break" -> breakTile(index, tile, fluidPos(index));
                    case "place" -> placeFluid(index, held.get(index));
                    default -> throw new IllegalArgumentException(action);
                }
                return;
            }
            TileEntityChemicalTank tile = chemicalTank();
            ConfigInfo info = tile == null ? null : tile.getConfig().getConfig(TransmissionType.CHEMICAL);
            switch (part[0]) {
                case "chem" -> setChemical(tile, part[1], part.length > 2 ? Long.parseLong(part[2]) : 0);
                case "recv" -> setChemical(receiver(Direction.byName(part[1])), part[2], Long.parseLong(part[3]));
                case "dump" -> tile.nextMode(0);
                case "eject" -> info.setEjecting(Boolean.parseBoolean(part[1]));
                case "side" -> {
                    RelativeSide side = relativeSide(part[1]);
                    info.setDataType(dataType(part[2]), side);
                    tile.getConfig().sideChanged(TransmissionType.CHEMICAL, side);
                }
                case "drainslot" -> setSlot(tile.getInventorySlots().get(0), stackOf(part[1]));
                case "fillslot" -> setSlot(tile.getInventorySlots().get(1), stackOf(part[1]));
                case "push", "pull" -> {
                    ResourceHandler<ChemicalResource> handler = level().getCapability(Capabilities.CHEMICAL.block(), helper.absolutePos(CHEMICAL_POS), sideOf(part[1]));
                    int moved = -1;
                    if (handler != null) {
                        ChemicalResource resource = chemicalOf(part[2]);
                        try (Transaction transaction = Transaction.openRoot()) {
                            moved = part[0].equals("push") ? handler.insert(resource, Integer.parseInt(part[3]), transaction) : handler.extract(resource, Integer.parseInt(part[3]), transaction);
                            transaction.commit();
                        }
                    }
                    results.add(moved);
                }
                case "break" -> breakTile(0, tile, helper.absolutePos(CHEMICAL_POS));
                case "place" -> placeChemical(held.get(0));
                default -> throw new IllegalArgumentException(action);
            }
        }

        static RelativeSide relativeSide(String name) {
            for (RelativeSide side : RelativeSide.VALUES) {
                if (side.getSerializedName().equals(name)) {
                    return side;
                }
            }
            throw new IllegalArgumentException(name);
        }

        static DataType dataType(String name) {
            for (DataType type : DataType.values()) {
                if (type.getSerializedName().equals(name)) {
                    return type;
                }
            }
            throw new IllegalArgumentException(name);
        }

        void begin(MekGameTestHelper helper) {
            this.helper = helper;
            this.json = RegistryOps.create(JsonOps.INSTANCE, helper.getLevel().registryAccess());
            JsonObject meta = new JsonObject();
            meta.addProperty("commit", COMMIT);
            meta.addProperty("date", DATE);
            meta.addProperty("file", "tank_" + kind + "_" + name + ".jsonl");
            meta.addProperty("scenario", description);
            JsonArray tanks = new JsonArray();
            if (kind.equals("fluid")) {
                for (int i = 0; i < fluidTiers.size(); i++) {
                    placeFluid(i, new ItemStack(fluidTankBlock(fluidTiers.get(i)).asItem()));
                    TileEntityFluidTank tile = fluidTank(i);
                    JsonObject tank = new JsonObject();
                    tank.addProperty("tier", fluidTiers.get(i));
                    tank.addProperty("capacity", tile.tier.getCapacity());
                    tank.addProperty("rate", tile.tier.getTransferRate());
                    tank.addProperty("facing", tile.getDirection().getSerializedName());
                    tanks.add(tank);
                }
                meta.addProperty("fields", "t; marks (the actions of the tick, run in ServerTickEvent.Pre); results (what each push and pull moved, in mark order; -1 with no capability on that side); "
                      + "tanks, bottom first: fluid, amount, scale (prevScale), comparator (getCurrentRedstoneLevel), active, mode (ContainerEditMode), valve (getValveFluid), light (lightUpdateDelay), "
                      + "in and out as [item, count, fluid of the item, mB, chemical of the item, mB] (null when the tile is gone); drops: {t, nbt (saveWithFullMetadata), stacks (ItemStack.CODEC)} of each break.");
            } else {
                placeChemical(new ItemStack(chemicalTankBlock(chemicalTier).asItem()));
                TileEntityChemicalTank tile = chemicalTank();
                JsonObject tank = new JsonObject();
                tank.addProperty("tier", chemicalTier);
                tank.addProperty("capacity", tile.getTier().getCapacity());
                tank.addProperty("rate", tile.getTier().getTransferRate());
                tank.addProperty("facing", tile.getDirection().getSerializedName());
                tanks.add(tank);
                JsonArray receivers = new JsonArray();
                for (int i = 0; i < receiverFaces.size(); i++) {
                    Direction face = receiverFaces.get(i);
                    helper.setBlock(CHEMICAL_POS.relative(face), chemicalTankBlock(receiverTiers.get(i)).defaultBlockState());
                    TileEntityChemicalTank other = receiver(face);
                    other.applyComponentsFromItemStack(new ItemStack(chemicalTankBlock(receiverTiers.get(i)).asItem()));
                    ConfigInfo info = ((ISideConfiguration) other).getConfig().getConfig(TransmissionType.CHEMICAL);
                    for (RelativeSide relative : RelativeSide.VALUES) {
                        info.setDataType(DataType.INPUT_OUTPUT, relative);
                        ((ISideConfiguration) other).getConfig().sideChanged(TransmissionType.CHEMICAL, relative);
                    }
                    info.setEjecting(false);
                    JsonObject receiver = new JsonObject();
                    receiver.addProperty("face", face.getSerializedName());
                    receiver.addProperty("tier", receiverTiers.get(i));
                    receiver.addProperty("capacity", other.getTier().getCapacity());
                    receivers.add(receiver);
                }
                meta.add("receivers", receivers);
                meta.addProperty("fields", "t; marks (the actions of the tick, run in ServerTickEvent.Pre); results (what each push and pull moved, in mark order; -1 with no capability on that side); "
                      + "tank: chemical, amount, comparator (getCurrentRedstoneLevel), dumping (GasMode), ejecting, sides (DataType per RelativeSide, in RelativeSide.VALUES order), "
                      + "drain (slot 0) and fill (slot 1) as [item, count, fluid of the item, mB, chemical of the item, mB] (null when the tile is gone); receivers {face: [chemical, amount]}; "
                      + "drops: {t, nbt (saveWithFullMetadata), stacks (ItemStack.CODEC)} of each break.");
            }
            meta.add("tanks", tanks);
            JsonObject metaLine = new JsonObject();
            metaLine.add("meta", meta);
            lines.add(metaLine.toString());
            armed = true;
        }

        void pre() {
            if (!armed || done) {
                return;
            }
            running = true;
            List<String> list = script.get(tick);
            if (list != null) {
                for (String action : list) {
                    marks.add(action);
                    apply(action);
                }
            }
        }

        void post() {
            if (!running || done) {
                return;
            }
            record();
            marks.clear();
            tick++;
            if (tick >= TICKS) {
                done = true;
            }
        }

        void record() {
            JsonObject line = new JsonObject();
            line.addProperty("t", tick);
            JsonArray markArray = new JsonArray();
            marks.forEach(markArray::add);
            line.add("marks", markArray);
            line.add("results", results.deepCopy());
            while (!results.isEmpty()) {
                results.remove(0);
            }
            if (kind.equals("fluid")) {
                JsonArray tanks = new JsonArray();
                for (int i = 0; i < fluidTiers.size(); i++) {
                    TileEntityFluidTank tile = fluidTank(i);
                    if (tile == null) {
                        tanks.add(com.google.gson.JsonNull.INSTANCE);
                        continue;
                    }
                    JsonObject out = new JsonObject();
                    out.addProperty("fluid", tile.fluidTank.isEmpty() ? "" : tile.fluidTank.resource().typeHolder().getRegisteredName());
                    out.addProperty("amount", tile.fluidTank.amountAsLong());
                    out.addProperty("scale", tile.prevScale);
                    out.addProperty("comparator", tile.getCurrentRedstoneLevel());
                    out.addProperty("active", tile.getActive());
                    out.addProperty("mode", tile.getContainerEditMode().getSerializedName());
                    out.addProperty("light", lightDelay(tile));
                    out.addProperty("valve", tile.getValveFluid().isEmpty() ? "" : tile.getValveFluid().typeHolder().getRegisteredName());
                    out.add("in", slotJson(tile.getInventorySlots().get(0)));
                    out.add("out", slotJson(tile.getInventorySlots().get(1)));
                    tanks.add(out);
                }
                line.add("tanks", tanks);
            } else {
                TileEntityChemicalTank tile = chemicalTank();
                if (tile == null) {
                    line.add("tank", com.google.gson.JsonNull.INSTANCE);
                } else {
                    JsonObject out = new JsonObject();
                    out.addProperty("chemical", tile.getChemicalTank().isEmpty() ? "" : tile.getChemicalTank().resource().typeHolder().getRegisteredName());
                    out.addProperty("amount", tile.getChemicalTank().amountAsLong());
                    out.addProperty("comparator", tile.getCurrentRedstoneLevel());
                    out.addProperty("dumping", tile.dumping.getSerializedName());
                    ConfigInfo info = tile.getConfig().getConfig(TransmissionType.CHEMICAL);
                    out.addProperty("ejecting", info.isEjecting());
                    JsonArray sides = new JsonArray();
                    for (RelativeSide side : RelativeSide.VALUES) {
                        sides.add(info.getDataType(side).getSerializedName());
                    }
                    out.add("sides", sides);
                    out.add("drain", slotJson(tile.getInventorySlots().get(0)));
                    out.add("fill", slotJson(tile.getInventorySlots().get(1)));
                    line.add("tank", out);
                }
                JsonObject receivers = new JsonObject();
                for (Direction face : receiverFaces) {
                    TileEntityChemicalTank other = receiver(face);
                    JsonArray entry = new JsonArray();
                    entry.add(other.getChemicalTank().isEmpty() ? "" : other.getChemicalTank().resource().typeHolder().getRegisteredName());
                    entry.add(other.getChemicalTank().amountAsLong());
                    receivers.add(face.getSerializedName(), entry);
                }
                line.add("receivers", receivers);
            }
            if (!drops.isEmpty()) {
                line.add("drops", drops.deepCopy());
                while (!drops.isEmpty()) {
                    drops.remove(0);
                }
            }
            lines.add(line.toString());
        }

        void finish() {
            try {
                Path file = Path.of("trace", "tank_" + kind + "_" + name + ".jsonl");
                Files.createDirectories(file.getParent());
                Files.write(file, lines);
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }
}
