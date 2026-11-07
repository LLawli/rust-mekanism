package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.common.Mekanism;
import mekanism.common.capabilities.energy.BasicEnergyContainer;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tile.base.TileEntityMekanism;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.minecraft.core.Direction;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.transfer.item.ItemResource;

/// O som da máquina no cliente oficial, para o vídeo: monta a máquina, liga (item na entrada), para (energia a zero) e tira um print a
/// cada 3 ticks com o instante real de cada um, em `cena-som-tempos.txt`. Só age com o arquivo `cena-som` na pasta do jogo, com
/// `maquina chamber|crusher|smelter|infuser` e `muffling N`. O áudio é gravado de fora, pelo PipeWire.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaSom {

    private static final Path ATIVA = Path.of("cena-som");
    private static final Path TEMPOS = Path.of("cena-som-tempos.txt");
    private static final int X = 300;
    private static final int Y = -60;
    private static final int Z = 3;
    private static final int PRINT_A_CADA = 3;
    private static final int LIGA = 80;
    private static final int PARA = 260;
    private static final int FIM = 480;

    private static String machine = "chamber";
    private static int muffling;
    private static boolean loaded;
    private static int ticks;
    private static int frame;
    private static long t0;
    private static final List<String> tempos = new ArrayList<>();

    private static void load() {
        try {
            for (String line : Files.readAllLines(ATIVA)) {
                String[] words = line.trim().split("\\s+");
                if (words.length >= 2 && words[0].equals("maquina")) {
                    machine = words[1];
                } else if (words.length >= 2 && words[0].equals("muffling")) {
                    muffling = Integer.parseInt(words[1]);
                }
            }
        } catch (IOException e) {
            Mekanism.logger.error("cena-som: sem arquivo", e);
        }
        loaded = true;
    }

    private static BlockState block() {
        BlockState state = switch (machine) {
            case "crusher" -> MekanismBlocks.CRUSHER.defaultState();
            case "smelter" -> MekanismBlocks.ENERGIZED_SMELTER.defaultState();
            case "infuser" -> MekanismBlocks.METALLURGIC_INFUSER.defaultState();
            default -> MekanismBlocks.ENRICHMENT_CHAMBER.defaultState();
        };
        return state.trySetValue(BlockStateProperties.HORIZONTAL_FACING, Direction.SOUTH);
    }

    private static void run(MinecraftServer server, String command) {
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command);
    }

    private static TileEntityMekanism tile(MinecraftServer server) {
        return (TileEntityMekanism) server.overworld().getBlockEntity(new BlockPos(X, Y, Z));
    }

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (minecraft.level == null || minecraft.player == null || server == null || !Files.exists(ATIVA)) {
            return;
        }
        if (!loaded) {
            load();
        }
        ticks++;
        if (ticks == 1) {
            // Só a máquina deve soar: a música e o ambiente ficam mudos, como no port.
            minecraft.options.getSoundSourceOptionInstance(net.minecraft.sounds.SoundSource.MUSIC).set(0.0);
            minecraft.options.getSoundSourceOptionInstance(net.minecraft.sounds.SoundSource.AMBIENT).set(0.0);
        }
        switch (ticks) {
            case 20 -> {
                server.execute(() -> run(server, "difficulty peaceful"));
                server.execute(() -> run(server, "time set 6000"));
                server.execute(() -> run(server, "weather clear"));
                server.execute(() -> run(server, "clear @a"));
            }
            case 30 -> server.execute(() -> {
                run(server, "fill " + (X - 8) + " -61 -8 " + (X + 8) + " -40 10 minecraft:air");
                server.overworld().setBlockAndUpdate(new BlockPos(X, Y, Z), Blocks.AIR.defaultBlockState());
                server.overworld().setBlockAndUpdate(new BlockPos(X, Y, Z), block());
                TileEntityMekanism tile = tile(server);
                tile.applyComponents(net.minecraft.core.component.DataComponentMap.EMPTY, net.minecraft.core.component.DataComponentPatch.EMPTY);
                if (muffling > 0) {
                    Holder<mekanism.api.upgrade.Upgrade> upgrade = server.overworld().registryAccess().getOrThrow(
                          net.minecraft.resources.ResourceKey.create(mekanism.api.MekanismRegistries.Keys.UPGRADES, Mekanism.rl("muffling")));
                    tile.getComponent().addUpgrades(server.overworld().registryAccess(), upgrade, muffling);
                }
                BasicEnergyContainer container = (BasicEnergyContainer) tile.getEnergyContainer();
                container.setEnergy(container.getCapacityAsLong(), null);
            });
            case 40 -> server.execute(() -> run(server, "tp @a 300.5 -59.0 0.5 0 12"));
            case 60 -> {
                t0 = System.nanoTime();
                tempos.add("epoch " + System.currentTimeMillis() / 1000.0);
            }
            default -> {
            }
        }
        if (ticks < 60) {
            return;
        }
        int t = ticks - 60;
        if (t == LIGA) {
            server.execute(() -> {
                TileEntityMekanism tile = tile(server);
                boolean infuser = machine.equals("infuser");
                String input = switch (machine) {
                    case "crusher" -> "minecraft:cobblestone";
                    case "smelter" -> "minecraft:raw_iron";
                    case "infuser" -> "minecraft:iron_ingot";
                    default -> "minecraft:coal";
                };
                var item = BuiltInRegistries.ITEM.getValue(Identifier.parse(input));
                tile.getInventorySlots().get(infuser ? 1 : 0).setContents(ItemResource.of(item), 64, null);
                if (infuser) {
                    var coal = BuiltInRegistries.ITEM.getValue(Identifier.parse("minecraft:coal"));
                    tile.getInventorySlots().get(0).setContents(ItemResource.of(coal), 64, null);
                }
            });
        } else if (t == PARA) {
            server.execute(() -> {
                BasicEnergyContainer container = (BasicEnergyContainer) tile(server).getEnergyContainer();
                container.setEnergy(0, null);
            });
        }
        if (t % PRINT_A_CADA == 0 && t <= FIM) {
            String name = String.format("oficial_som_%05d", frame);
            tempos.add(frame + " " + (System.nanoTime() - t0) / 1_000_000_000.0);
            Screenshot.grab(minecraft.gameDirectory, name + ".png", minecraft.gameRenderer.mainRenderTarget(), 1, message -> {
            });
            frame++;
        }
        if (t == FIM + 10) {
            try {
                Files.write(TEMPOS, tempos);
            } catch (IOException e) {
                Mekanism.logger.error("cena-som: tempos", e);
            }
            minecraft.stop();
        }
    }
}
