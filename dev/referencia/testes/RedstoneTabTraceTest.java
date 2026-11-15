package mekanism.common.tests.network;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.serialization.JsonOps;
import java.io.IOException;
import java.lang.reflect.Proxy;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Supplier;
import mekanism.api.security.IBlockSecurityUtils;
import mekanism.api.security.SecurityMode;
import mekanism.common.network.to_server.PacketGuiInteract;
import mekanism.common.network.to_server.PacketGuiInteract.GuiInteraction;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.interfaces.IRedstoneControl.RedstoneControl;
import mekanism.generators.common.registries.GeneratorsBlocks;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.RegistryAccess;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.resources.RegistryOps;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.ItemLike;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.network.handling.IPayloadContext;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava a aba de controle de redstone (`GuiRedstoneControlTab`) de uma máquina posta por um jogador: `redstone_<máquina>.jsonl`, uma linha por modo de partida. O clique é o
/// pacote de verdade, `PacketGuiInteract(NEXT_REDSTONE_CONTROL ou PREVIOUS_REDSTONE_CONTROL).handle`, com o jogador que clica (o dono, e outro jogador com a máquina pública, privada
/// e confiável): o modo que sobra, o NBT do tile, o item solto e o modo do tile posto de volta, o estado do tile com e sem sinal (`canFunction`, `isRedstoneActivated`,
/// `isPowered`, `wasPowered`), e a volta inteira dos modos por cliques seguidos.
@ForEachTest(groups = "trace")
public class RedstoneTabTraceTest {

    private static final int TIMEOUT = 60 * SharedConstants.TICKS_PER_SECOND;
    private static final BlockPos SPOT = new BlockPos(3, 1, 1);
    private static final BlockPos FLOOR = new BlockPos(3, 0, 1);
    private static final BlockPos SIGNAL = new BlockPos(3, 1, 0);

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

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_redstone_tab")
    @TestHolder(description = "Records the redstone control tab of the Crusher: the click packet for each player, the modes, the nbt and the item.")
    public static void crusher(final DynamicTest test) {
        template(test, CRUSHER);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_redstone_tab")
    @TestHolder(description = "Records the redstone control tab of the Heat Generator: the click packet for each player, the modes, the nbt and the item.")
    public static void heat(final DynamicTest test) {
        template(test, HEAT);
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_redstone_tab")
    @TestHolder(description = "Records the redstone control tab of the Basic Energy Cube: the click packet for each player, the modes, the nbt and the item.")
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
            for (RedstoneControl start : RedstoneControl.values()) {
                lines.add(line(start).toString());
            }
            write("redstone_" + spec.name, lines);
        }

        private void clear() {
            helper.setBlock(SPOT, Blocks.AIR.defaultBlockState());
            helper.setBlock(SIGNAL, Blocks.AIR.defaultBlockState());
        }

        /// A máquina posta pelo dono, no modo de segurança `security` e no de redstone `control` (`setControlType`, que troca o que a máquina não aceita).
        private TileEntityMekanism place(SecurityMode security, RedstoneControl control) {
            clear();
            helper.useOn(FLOOR, new ItemStack(spec.item.get()), owner, Direction.UP);
            TileEntityMekanism tile = tile();
            tile.getSecurity().setMode(security);
            tile.setControlType(control);
            return tile;
        }

        private TileEntityMekanism tile() {
            BlockEntity entity = helper.getLevel().getBlockEntity(helper.absolutePos(SPOT));
            return (TileEntityMekanism) entity;
        }

        /// O pacote de verdade: `PacketGuiInteract.handle` com um contexto que só sabe dizer quem é o jogador (`PacketUtils.blockEntity` confere o acesso e o `GuiInteraction` troca o modo).
        private void click(Player player, TileEntityMekanism tile, GuiInteraction interaction) {
            IPayloadContext context = (IPayloadContext) Proxy.newProxyInstance(IPayloadContext.class.getClassLoader(), new Class<?>[]{IPayloadContext.class}, (proxy, method, args) -> {
                if (method.getName().equals("player")) {
                    return player;
                }
                throw new UnsupportedOperationException(method.getName());
            });
            new PacketGuiInteract(interaction, tile).handle(context);
        }

        private String after(Player player, SecurityMode security, RedstoneControl start, boolean forward) {
            TileEntityMekanism tile = place(security, start);
            click(player, tile, forward ? GuiInteraction.NEXT_REDSTONE_CONTROL : GuiInteraction.PREVIOUS_REDSTONE_CONTROL);
            return tile.getControlType().name();
        }

        private JsonObject state(TileEntityMekanism tile) {
            JsonObject out = new JsonObject();
            out.addProperty("control", tile.getControlType().name());
            out.addProperty("powered", tile.isPowered());
            out.addProperty("was_powered", tile.wasPowered());
            out.addProperty("activated", tile.isRedstoneActivated());
            out.addProperty("can_function", tile.canFunction());
            return out;
        }

        private JsonObject line(RedstoneControl start) {
            ServerLevel level = helper.getLevel();
            BlockPos abs = helper.absolutePos(SPOT);
            JsonObject root = new JsonObject();
            root.addProperty("start", start.name());
            root.addProperty("effective_start", place(SecurityMode.PUBLIC, start).getControlType().name());
            root.addProperty("owner", owner.getUUID().toString());
            root.addProperty("other", other.getUUID().toString());
            JsonObject clicks = new JsonObject();
            for (boolean forward : new boolean[]{true, false}) {
                String way = forward ? "next" : "previous";
                clicks.addProperty("owner_" + way, after(owner, SecurityMode.PUBLIC, start, forward));
                for (SecurityMode security : SecurityMode.values()) {
                    clicks.addProperty("other_" + security.name().toLowerCase() + "_" + way, after(other, security, start, forward));
                }
                clicks.addProperty("owner_private_" + way, after(owner, SecurityMode.PRIVATE, start, forward));
            }
            root.add("clicks", clicks);
            JsonObject access = new JsonObject();
            for (SecurityMode security : SecurityMode.values()) {
                TileEntityMekanism tile = place(security, start);
                access.addProperty("owner_" + security.name().toLowerCase(), IBlockSecurityUtils.INSTANCE.canAccess(owner, level, abs, tile));
                access.addProperty("other_" + security.name().toLowerCase(), IBlockSecurityUtils.INSTANCE.canAccess(other, level, abs, tile));
            }
            root.add("access", access);
            JsonArray around = new JsonArray();
            TileEntityMekanism tile = place(SecurityMode.PUBLIC, RedstoneControl.DISABLED);
            around.add(tile.getControlType().name());
            for (int i = 0; i < 6; i++) {
                click(owner, tile, GuiInteraction.NEXT_REDSTONE_CONTROL);
                around.add(tile.getControlType().name());
            }
            root.add("forward_cycle", around);
            JsonArray back = new JsonArray();
            tile = place(SecurityMode.PUBLIC, RedstoneControl.DISABLED);
            back.add(tile.getControlType().name());
            for (int i = 0; i < 6; i++) {
                click(owner, tile, GuiInteraction.PREVIOUS_REDSTONE_CONTROL);
                back.add(tile.getControlType().name());
            }
            root.add("backward_cycle", back);
            // O que o clique deixa no tile: o NBT e o item solto, e o modo do tile que o item põe de volta.
            tile = place(SecurityMode.PUBLIC, start);
            click(owner, tile, GuiInteraction.NEXT_REDSTONE_CONTROL);
            CompoundTag tag = tile.saveWithFullMetadata(registries);
            JsonObject nbt = NbtOps.INSTANCE.convertTo(JsonOps.INSTANCE, tag).getAsJsonObject();
            root.add("nbt_control_type", nbt.get("control_type") == null ? null : nbt.get("control_type"));
            BlockState state = level.getBlockState(abs);
            ItemStack dropped = null;
            for (ItemStack drop : Block.getDrops(state, level, abs, tile)) {
                dropped = drop;
                root.add("drop", ItemStack.CODEC.encodeStart(json, drop).getOrThrow());
            }
            if (dropped != null) {
                clear();
                helper.useOn(FLOOR, dropped.copy(), owner, Direction.UP);
                root.addProperty("placed_back", tile().getControlType().name());
                clear();
            }
            // O efeito no tile, sem sinal e com um bloco de redstone ao lado, depois de cada clique do dono (o que `canFunction` decide a cada tick).
            JsonObject effect = new JsonObject();
            for (boolean powered : new boolean[]{false, true}) {
                tile = place(SecurityMode.PUBLIC, start);
                if (powered) {
                    helper.setBlock(SIGNAL, Blocks.REDSTONE_BLOCK.defaultBlockState());
                }
                JsonArray steps = new JsonArray();
                steps.add(state(tile));
                for (int i = 0; i < 3; i++) {
                    click(owner, tile, GuiInteraction.NEXT_REDSTONE_CONTROL);
                    steps.add(state(tile));
                }
                effect.add(powered ? "signal" : "no_signal", steps);
            }
            root.add("effect", effect);
            clear();
            return root;
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
