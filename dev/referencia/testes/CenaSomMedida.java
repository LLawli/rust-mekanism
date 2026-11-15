package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import mekanism.common.Mekanism;
import mekanism.common.capabilities.energy.BasicEnergyContainer;
import mekanism.common.config.MekanismConfig;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.minecraft.client.Minecraft;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.MinecraftServer;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.event.sound.PlaySoundEvent;
import net.neoforged.neoforge.transfer.item.ItemResource;

/// O som da máquina no cliente oficial, medido sem ouvir: roda uma lista de cenários e escreve `som-medido.jsonl` na pasta do jogo. Só age com o
/// arquivo `cena-som-medida` ao lado, uma linha por cenário, em pares `chave=valor`:
///
///   nome=padrao maquina=chamber muffling=0 base=1.0 enable=true jogador=3,-0.5,0 liga=20 desliga=200 fim=260 ev=100:muffling:0 ev=150:base:0.5
///
/// `jogador` é onde ficam os PÉS do jogador, em relação ao centro do bloco da máquina (a posição do som). `ev=T:chave:valor` muda `muffling`,
/// `base` (`baseSoundVolume`), `enable` (`enableMachineSounds`), `jogador` (`dx;dy;dz`) ou `tick` (`freeze`, `unfreeze`) no tick T do cenário. Cada tick do cliente gera uma linha
/// `tick` com o que o tile e o som valem naquele instante, e cada som de tile que o motor começa gera uma linha `play` com os campos da instância.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaSomMedida {

    private static final Path ATIVA = Path.of("cena-som-medida");
    /// Gravação de áudio (`oficial_som_audio.sh`): com `som-audio-espera` ao lado, o volume mestre fica em 0 até `som-audio-ok` aparecer (o stream do jogo já está no sink nulo e
    /// o gravador de pé), e `som-audio-mudo` o leva de volta a 0 na hora. Sem `som-audio-espera` a cena não mexe no volume mestre.
    private static final Path ESPERA_AUDIO = Path.of("som-audio-espera");
    private static final Path AUDIO_OK = Path.of("som-audio-ok");
    private static final Path AUDIO_MUDO = Path.of("som-audio-mudo");
    private static boolean audioLiberado;
    private static int esperaAudio;
    private static final Path SAIDA = Path.of("som-medido.jsonl");
    private static final int X = 300;
    private static final int Y = -60;
    private static final int Z = 3;
    private static final double CX = X + 0.5;
    private static final double CY = Y + 0.5;
    private static final double CZ = Z + 0.5;
    /// Ticks de espera entre dois cenários, com o bloco já tirado.
    private static final int PAUSA = 10;

    private static final class Cenario {
        String nome = "x";
        String maquina = "chamber";
        int muffling;
        double base = 1.0;
        boolean enable = true;
        double[] jogador = {3.0, -0.5, 0.0};
        int liga = 20;
        int desliga = 200;
        int fim = 260;
        final List<String[]> eventos = new ArrayList<>();
    }

    private static final List<Cenario> cenarios = new ArrayList<>();
    private static boolean loaded;
    private static boolean done;
    private static int ticks;
    private static int indice = -1;
    private static int n;
    private static int pausa;
    private static Cenario atual;

    private static void escreve(String linha) {
        try {
            Files.writeString(SAIDA, linha + "\n", StandardOpenOption.CREATE, StandardOpenOption.APPEND);
        } catch (IOException e) {
            Mekanism.logger.error("cena-som-medida: escrita", e);
        }
    }

    private static void load() {
        try {
            for (String line : Files.readAllLines(ATIVA)) {
                line = line.trim();
                if (line.isEmpty() || line.startsWith("#")) {
                    continue;
                }
                Cenario c = new Cenario();
                for (String word : line.split("\\s+")) {
                    String[] kv = word.split("=", 2);
                    if (kv.length < 2) {
                        continue;
                    }
                    switch (kv[0]) {
                        case "nome" -> c.nome = kv[1];
                        case "maquina" -> c.maquina = kv[1];
                        case "muffling" -> c.muffling = Integer.parseInt(kv[1]);
                        case "base" -> c.base = Double.parseDouble(kv[1]);
                        case "enable" -> c.enable = Boolean.parseBoolean(kv[1]);
                        case "jogador" -> c.jogador = parseVec(kv[1], ",");
                        case "liga" -> c.liga = Integer.parseInt(kv[1]);
                        case "desliga" -> c.desliga = Integer.parseInt(kv[1]);
                        case "fim" -> c.fim = Integer.parseInt(kv[1]);
                        case "ev" -> c.eventos.add(kv[1].split(":", 3));
                        default -> {
                        }
                    }
                }
                cenarios.add(c);
            }
        } catch (IOException e) {
            Mekanism.logger.error("cena-som-medida: sem arquivo", e);
        }
        loaded = true;
        try {
            Files.deleteIfExists(SAIDA);
        } catch (IOException e) {
            Mekanism.logger.error("cena-som-medida: limpar", e);
        }
    }

    private static double[] parseVec(String text, String sep) {
        String[] p = text.split(sep);
        return new double[]{Double.parseDouble(p[0]), Double.parseDouble(p[1]), Double.parseDouble(p[2])};
    }

    private static BlockState block(String machine) {
        BlockState state = switch (machine) {
            case "crusher" -> MekanismBlocks.CRUSHER.defaultState();
            case "smelter" -> MekanismBlocks.ENERGIZED_SMELTER.defaultState();
            case "infuser" -> MekanismBlocks.METALLURGIC_INFUSER.defaultState();
            case "heat_generator" -> GeneratorsBlocks.HEAT_GENERATOR.defaultState();
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

    private static void setMuffling(MinecraftServer server, int count) {
        TileEntityMekanism tile = tile(server);
        Holder<mekanism.api.upgrade.Upgrade> upgrade = server.overworld().registryAccess().getOrThrow(
              net.minecraft.resources.ResourceKey.create(mekanism.api.MekanismRegistries.Keys.UPGRADES, Mekanism.rl("muffling")));
        int have = tile.getComponent().getUpgrades(upgrade);
        if (count > have) {
            tile.getComponent().addUpgrades(server.overworld().registryAccess(), upgrade, count - have);
        } else if (count < have) {
            // Como `PacketRemoveUpgrade`: o botão de remover da tela de upgrades.
            tile.getComponent().removeUpgrade(server.overworld().registryAccess(), upgrade, count == 0);
        }
    }

    /// Liga a máquina (item na entrada e energia cheia) ou a desliga (energia a zero), como o `CenaSom`.
    private static void energia(MinecraftServer server, Cenario c, boolean liga) {
        TileEntityMekanism tile = tile(server);
        BasicEnergyContainer container = (BasicEnergyContainer) tile.getEnergyContainer();
        if (tile instanceof TileEntityHeatGenerator generator) {
            // O Heat Generator liga com lava no tanque e um Energy Tablet vazio no slot de energia, que leva o buffer a cada tick (sem ele o buffer enche e o gerador para); desliga sem lava.
            if (liga) {
                generator.lavaTank.setContents(FluidResource.of(Fluids.LAVA), 1000, null);
                generator.getInventorySlots().get(1).setContents(ItemResource.of(mekanism.common.registries.MekanismItems.ENERGY_TABLET.asItem()), 1, null);
            } else {
                generator.lavaTank.setContents(FluidResource.EMPTY, 0, null);
            }
            return;
        }
        if (!liga) {
            container.setEnergy(0, null);
            return;
        }
        boolean infuser = c.maquina.equals("infuser");
        String input = switch (c.maquina) {
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
        container.setEnergy(container.getCapacityAsLong(), null);
    }

    private static void teleport(MinecraftServer server, double[] feet) {
        run(server, String.format(Locale.ROOT, "tp @a %.4f %.4f %.4f 0 12", CX + feet[0], CY + feet[1], CZ + feet[2]));
    }

    @SuppressWarnings("unchecked")
    private static SoundInstance tileSound() {
        try {
            Class<?> handler = Class.forName("mekanism.client.sound.SoundHandler");
            Field field = handler.getDeclaredField("soundMap");
            field.setAccessible(true);
            var map = (it.unimi.dsi.fastutil.longs.Long2ObjectMap<SoundInstance>) field.get(null);
            return map.get(new BlockPos(X, Y, Z).asLong());
        } catch (ReflectiveOperationException e) {
            return null;
        }
    }

    private static Object field(Object target, String name) {
        try {
            Field field = target.getClass().getDeclaredField(name);
            field.setAccessible(true);
            return field.get(target);
        } catch (ReflectiveOperationException e) {
            return null;
        }
    }

    private static boolean fullyMuffled(TileEntityMekanism tile) {
        try {
            Method method = TileEntityMekanism.class.getDeclaredMethod("isFullyMuffled");
            method.setAccessible(true);
            return (boolean) method.invoke(tile);
        } catch (ReflectiveOperationException e) {
            return false;
        }
    }

    private static String num(double value) {
        return String.format(Locale.ROOT, "%.4f", value);
    }

    /// Os sons de bloco do Mekanism que o motor começa: a instância do `TileTickableSound` com todos os campos que a legenda e o `OpenAL` leem.
    @SubscribeEvent
    public static void onPlay(PlaySoundEvent event) {
        if (atual == null || !event.getName().startsWith("tile.")) {
            return;
        }
        SoundInstance sound = event.getOriginalSound();
        if (!sound.getIdentifier().getNamespace().startsWith("mekanism")) {
            return;
        }
        Object checkInterval = field(sound, "checkInterval");
        escreve(String.format(Locale.ROOT,
              "{\"e\":\"play\",\"cenario\":\"%s\",\"n\":%d,\"nome\":\"%s\",\"id\":\"%s\",\"categoria\":\"%s\",\"volume\":%s,\"pitch\":%s,"
              + "\"atenuacao\":\"%s\",\"laco\":%s,\"atraso\":%d,\"x\":%s,\"y\":%s,\"z\":%s,\"relativo\":%s,\"comeca_mudo\":%s,\"distancia_atenuacao\":%d,\"intervalo\":%s}",
              atual.nome, n, event.getName(), sound.getIdentifier(), sound.getSource().getName(), num(sound.getVolume()), num(sound.getPitch()),
              sound.getAttenuation(), sound.isLooping(), sound.getDelay(), num(sound.getX()), num(sound.getY()), num(sound.getZ()), sound.isRelative(),
              sound.canStartSilent(), sound.getSound() == null ? -1 : sound.getSound().getAttenuationDistance(), checkInterval));
    }

    private static void aplicaEvento(MinecraftServer server, String[] ev) {
        int quando = Integer.parseInt(ev[0]);
        if (quando != n) {
            return;
        }
        switch (ev[1]) {
            case "muffling" -> server.execute(() -> setMuffling(server, Integer.parseInt(ev[2])));
            case "base" -> MekanismConfig.client.baseSoundVolume.set(Double.parseDouble(ev[2]));
            case "enable" -> MekanismConfig.client.enableMachineSounds.set(Boolean.parseBoolean(ev[2]));
            case "jogador" -> atual.jogador = parseVec(ev[2], ";");
            case "tick" -> server.execute(() -> run(server, "tick " + ev[2]));
            case "energia" -> server.execute(() -> energia(server, atual, ev[2].equals("1")));
            default -> {
            }
        }
        escreve(String.format(Locale.ROOT, "{\"e\":\"evento\",\"cenario\":\"%s\",\"n\":%d,\"que\":\"%s\",\"valor\":\"%s\"}", atual.nome, n, ev[1], ev[2]));
    }

    private static void snapshot(Minecraft minecraft) {
        BlockEntity be = minecraft.level.getBlockEntity(new BlockPos(X, Y, Z));
        String tileJson = "null";
        if (be instanceof TileEntityMekanism tile) {
            tileJson = String.format(Locale.ROOT, "{\"lit\":%s,\"fator\":%s,\"mudo\":%s}", tile.getActive(), num(tile.getVolumeFactor()), fullyMuffled(tile));
        }
        SoundInstance sound = tileSound();
        String soundJson = "null";
        if (sound != null) {
            soundJson = String.format(Locale.ROOT, "{\"ativo\":%s,\"parado\":%s,\"volume\":%s,\"original\":%s,\"intervalo\":%s}",
                  minecraft.getSoundManager().isActive(sound), sound instanceof net.minecraft.client.resources.sounds.TickableSoundInstance t && t.isStopped(),
                  num(sound.getVolume()), field(sound, "originalVolume"), field(sound, "checkInterval"));
        }
        var player = minecraft.player.position();
        String linha = String.format(Locale.ROOT,
              "{\"e\":\"tick\",\"cenario\":\"%s\",\"n\":%d,\"gt\":%d,\"normal\":%s,\"tile\":%s,\"som\":%s,\"pes\":[%s,%s,%s],\"base\":%s,\"liga\":%s}",
              atual.nome, n, minecraft.level.getGameTime(), minecraft.level.tickRateManager().runsNormally(), tileJson, soundJson, num(player.x), num(player.y), num(player.z),
              num(MekanismConfig.client.baseSoundVolume.get()), MekanismConfig.client.enableMachineSounds.get());
        escreve(linha);
    }

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (done || minecraft.level == null || minecraft.player == null || server == null || !Files.exists(ATIVA)) {
            return;
        }
        if (!loaded) {
            load();
        }
        ticks++;
        if (ticks == 1) {
            minecraft.options.getSoundSourceOptionInstance(SoundSource.MUSIC).set(0.0);
            minecraft.options.getSoundSourceOptionInstance(SoundSource.AMBIENT).set(0.0);
        }
        if (ticks == 20) {
            server.execute(() -> run(server, "difficulty peaceful"));
            server.execute(() -> run(server, "time set 6000"));
            server.execute(() -> run(server, "weather clear"));
            server.execute(() -> run(server, "gamemode creative @a"));
            server.execute(() -> run(server, "clear @a"));
        }
        if (ticks < 40) {
            return;
        }
        if (Files.exists(ESPERA_AUDIO)) {
            if (Files.exists(AUDIO_MUDO)) {
                minecraft.options.getSoundSourceOptionInstance(SoundSource.MASTER).set(0.0);
                audioLiberado = false;
            } else if (!audioLiberado) {
                if (!Files.exists(AUDIO_OK)) {
                    // Cinco minutos sem o sink nulo: desiste sem tocar nada.
                    if (++esperaAudio > 20 * 300) {
                        escreve("{\"e\":\"fim\",\"erro\":\"sem sink nulo\"}");
                        done = true;
                        minecraft.stop();
                    }
                    return;
                }
                minecraft.options.getSoundSourceOptionInstance(SoundSource.MASTER).set(1.0);
                audioLiberado = true;
            }
            if (!audioLiberado) {
                return;
            }
        }
        if (atual == null) {
            if (pausa > 0) {
                pausa--;
                return;
            }
            indice++;
            if (indice >= cenarios.size()) {
                done = true;
                escreve("{\"e\":\"fim\"}");
                minecraft.options.getSoundSourceOptionInstance(SoundSource.MASTER).set(0.0);
                minecraft.stop();
                return;
            }
            atual = cenarios.get(indice);
            n = 0;
            final Cenario c = atual;
            MekanismConfig.client.baseSoundVolume.set(c.base);
            MekanismConfig.client.enableMachineSounds.set(c.enable);
            server.execute(() -> {
                run(server, "fill " + (X - 8) + " -61 -8 " + (X + 8) + " -40 10 minecraft:air");
                server.overworld().setBlockAndUpdate(new BlockPos(X, Y, Z), Blocks.AIR.defaultBlockState());
                server.overworld().setBlockAndUpdate(new BlockPos(X, Y, Z), block(c.maquina));
                TileEntityMekanism tile = tile(server);
                tile.applyComponents(net.minecraft.core.component.DataComponentMap.EMPTY, net.minecraft.core.component.DataComponentPatch.EMPTY);
                if (c.muffling > 0) {
                    setMuffling(server, c.muffling);
                }
                BasicEnergyContainer container = (BasicEnergyContainer) tile.getEnergyContainer();
                container.setEnergy(0, null);
                teleport(server, c.jogador);
            });
            escreve(String.format(Locale.ROOT, "{\"e\":\"cenario\",\"nome\":\"%s\",\"maquina\":\"%s\",\"muffling\":%d,\"base\":%s,\"enable\":%s}",
                  c.nome, c.maquina, c.muffling, num(c.base), c.enable));
            return;
        }
        final Cenario c = atual;
        n++;
        server.execute(() -> teleport(server, c.jogador));
        for (String[] ev : c.eventos) {
            aplicaEvento(server, ev);
        }
        if (n == c.liga) {
            server.execute(() -> energia(server, c, true));
            escreve(String.format(Locale.ROOT, "{\"e\":\"evento\",\"cenario\":\"%s\",\"n\":%d,\"que\":\"liga\",\"valor\":\"\"}", c.nome, n));
        }
        if (n == c.desliga) {
            server.execute(() -> energia(server, c, false));
            escreve(String.format(Locale.ROOT, "{\"e\":\"evento\",\"cenario\":\"%s\",\"n\":%d,\"que\":\"desliga\",\"valor\":\"\"}", c.nome, n));
        }
        snapshot(minecraft);
        if (n >= c.fim) {
            escreve(String.format(Locale.ROOT, "{\"e\":\"cenario_fim\",\"nome\":\"%s\"}", c.nome));
            server.execute(() -> server.overworld().setBlockAndUpdate(new BlockPos(X, Y, Z), Blocks.AIR.defaultBlockState()));
            atual = null;
            pausa = PAUSA;
        }
    }
}
