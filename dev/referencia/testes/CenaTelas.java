package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.chemical.ChemicalIds;
import mekanism.common.Mekanism;
import mekanism.common.capabilities.energy.BasicEnergyContainer;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.MekanismTests;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.machine.TileEntityMetallurgicInfuser;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.client.Minecraft;
import net.minecraft.client.MouseHandler;
import net.minecraft.client.Screenshot;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.api.distmarker.Dist;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.fml.common.EventBusSubscriber;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.neoforge.transfer.item.ItemResource;

/// As telas das máquinas e do Heat Generator no cliente oficial, nos mesmos estados que o port recebe, para o
/// `comparar_tela.py`. Só age com o arquivo `cena-telas` na pasta do jogo. Uma linha `escala N` define o `guiScale` dos casos
/// seguintes, e uma linha `caso NOME MAQUINA chave=valor...` um estado: `energia` (número ou `cap`), `entrada` e `saida`
/// (`item:quantidade`), `quimico` (`bio:500`), `lava`, `passos` (ticks do servidor depois de montar), `hover` (`x,y` em pixels de
/// GUI relativos à tela) e `capturas` (quantos prints, um a cada 2 ticks).
///
/// O servidor fica parado (`tick freeze`) enquanto o estado é montado e avança `passos` ticks com `tick step`, o que dá o mesmo
/// progresso nos dois jogos.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaTelas {

    private static final Path ATIVA = Path.of("cena-telas");
    private static final int Y = -60;
    private static final int Z = 3;
    private static final int OPEN_WAIT = 60;

    private record Caso(String name, int scale, String machine, Map<String, String> args, int x) {
    }

    private static List<Caso> CASOS;
    private static int ticks;
    private static int caseIndex;
    private static int caseTick;
    private static int shotsTaken;
    private static int appliedScale = -1;

    private static void load() {
        CASOS = new ArrayList<>();
        try {
            int scale = 2;
            int x = 300;
            for (String line : Files.readAllLines(ATIVA)) {
                String[] words = line.trim().split("\\s+");
                if (words.length < 2 || words[0].startsWith("#")) {
                    continue;
                }
                if (words[0].equals("escala")) {
                    scale = Integer.parseInt(words[1]);
                } else if (words[0].equals("caso") && words.length >= 3) {
                    Map<String, String> args = new HashMap<>();
                    for (int i = 3; i < words.length; i++) {
                        String[] pair = words[i].split("=", 2);
                        args.put(pair[0], pair.length > 1 ? pair[1] : "");
                    }
                    CASOS.add(new Caso(words[1], scale, words[2], args, x));
                }
            }
        } catch (IOException e) {
            Mekanism.logger.error("cena: sem lista", e);
        }
    }

    private static BlockState block(String machine) {
        return switch (machine) {
            case "chamber" -> MekanismBlocks.ENRICHMENT_CHAMBER.defaultState();
            case "crusher" -> MekanismBlocks.CRUSHER.defaultState();
            case "smelter" -> MekanismBlocks.ENERGIZED_SMELTER.defaultState();
            case "infuser" -> MekanismBlocks.METALLURGIC_INFUSER.defaultState();
            case "generator" -> GeneratorsBlocks.HEAT_GENERATOR.defaultState();
            default -> throw new IllegalArgumentException(machine);
        };
    }

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (minecraft.level == null || minecraft.player == null || server == null || !Files.exists(ATIVA)) {
            return;
        }
        if (CASOS == null) {
            load();
        }
        if (++ticks < 60) {
            if (ticks == 20) {
                server.execute(() -> run(server, "difficulty peaceful"));
                server.execute(() -> run(server, "time set 6000"));
                server.execute(() -> run(server, "weather clear"));
                server.execute(() -> run(server, "clear @a"));
            }
            return;
        }
        if (caseIndex >= CASOS.size()) {
            if (++caseTick > 40) {
                minecraft.stop();
            }
            return;
        }
        Caso caso = CASOS.get(caseIndex);
        int steps = Integer.parseInt(caso.args().getOrDefault("passos", "2"));
        int cycle = Integer.parseInt(caso.args().getOrDefault("ciclo", "0"));
        int captures = cycle > 0 ? cycle : Integer.parseInt(caso.args().getOrDefault("capturas", "1"));
        // t0: escala, parar o servidor e montar. t10: estado. t20: avançar. Depois, esperar o fim, abrir, pôr o cursor e tirar os prints.
        switch (caseTick++) {
            case 0 -> {
                if (appliedScale != caso.scale()) {
                    minecraft.options.guiScale().set(caso.scale());
                    minecraft.resizeGui();
                    appliedScale = caso.scale();
                }
                server.execute(() -> run(server, "tick freeze"));
            }
            case 5 -> server.execute(() -> build(server, caso));
            // Um tick para o bloco recalcular a capacidade (`recalculateUpgrades`), que nasce com 4 ticks de consumo.
            case 8 -> server.execute(() -> run(server, "tick step 1"));
            case 15 -> server.execute(() -> state(server, caso));
            case 17 -> server.execute(() -> run(server, "tp @a " + caso.x() + ".0 -60.0 0.0 0 0"));
            case 20 -> server.execute(() -> run(server, "tick step " + steps));
            default -> {
            }
        }
        // O `tick step` roda a 20 ticks por segundo, o mesmo ritmo do cliente: espera os passos e uma folga.
        int base = 25 + steps + 20;
        if (caseTick == base) {
            open(minecraft, caso);
        } else if (caseTick == base + OPEN_WAIT) {
            hideUnimplementedTabs(minecraft);
            hover(minecraft, caso);
            shotsTaken = 0;
        } else if (cycle > 0 && caseTick > base + OPEN_WAIT + 10 && shotsTaken < captures && (caseTick - base) % 6 == 0) {
            // Um passo do servidor por quadro: as animações do atlas só andam com o jogo rodando, e a fase de cada print é conhecida.
            server.execute(() -> run(server, "tick step 1"));
        } else if (cycle > 0 && caseTick > base + OPEN_WAIT + 10 && shotsTaken < captures && (caseTick - base) % 6 == 3) {
            shot(minecraft, "oficial_" + caso.name() + "_e" + caso.scale() + "_" + shotsTaken);
            shotsTaken++;
        } else if (cycle == 0 && caseTick > base + OPEN_WAIT + 10 && shotsTaken < captures && (caseTick - base) % 2 == 0) {
            String suffix = captures > 1 ? "_" + shotsTaken : "";
            shot(minecraft, "oficial_" + caso.name() + "_e" + caso.scale() + suffix);
            shotsTaken++;
        } else if (caseTick == base + OPEN_WAIT + 14 + captures * (cycle > 0 ? 6 : 2)) {
            minecraft.player.closeContainer();
            caseIndex++;
            caseTick = 0;
        }
    }

    private static void run(MinecraftServer server, String command) {
        server.getCommands().performPrefixedCommand(server.createCommandSourceStack(), command);
    }

    private static void shot(Minecraft minecraft, String name) {
        Screenshot.grab(minecraft.gameDirectory, name + ".png", minecraft.gameRenderer.mainRenderTarget(), 1,
              message -> Mekanism.logger.info("cena: {}", message.getString()));
    }

    /// O mundo atrás da tela é igual nos dois jogos: uma caixa oca de concreto preto em volta da máquina e do jogador, sem luz. O
    /// resto do que fica visível (o céu, o chão) mudaria de um jogo para o outro e apareceria nos cantos do painel e sob os tooltips.
    private static void box(MinecraftServer server, Caso caso) {
        int x = caso.x();
        run(server, "fill " + (x - 6) + " -61 -6 " + (x + 6) + " -50 10 minecraft:air");
        run(server, "fill " + (x - 6) + " -61 -6 " + (x + 6) + " -50 10 minecraft:black_concrete hollow");
    }

    private static void build(MinecraftServer server, Caso caso) {
        box(server, caso);
        ServerLevel level = server.overworld();
        level.setBlockAndUpdate(new BlockPos(caso.x(), Y, Z), Blocks.AIR.defaultBlockState());
        level.setBlockAndUpdate(new BlockPos(caso.x(), Y, Z), block(caso.machine()));
        // Como um jogador coloca a máquina: `applyImplicitComponents` recalcula os upgrades, e com eles a capacidade de energia.
        tile(server, caso).applyComponents(net.minecraft.core.component.DataComponentMap.EMPTY,
              net.minecraft.core.component.DataComponentPatch.EMPTY);
    }

    private static TileEntityMekanism tile(MinecraftServer server, Caso caso) {
        return (TileEntityMekanism) server.overworld().getBlockEntity(new BlockPos(caso.x(), Y, Z));
    }

    private static void setItem(TileEntityMekanism tile, int slot, String spec) {
        String[] parts = spec.split(":");
        String id = parts.length == 3 ? parts[0] + ":" + parts[1] : "minecraft:" + parts[0];
        int count = Integer.parseInt(parts[parts.length - 1]);
        var item = BuiltInRegistries.ITEM.getValue(Identifier.parse(id));
        tile.getInventorySlots().get(slot).setContents(ItemResource.of(item), count, null);
    }

    private static void state(MinecraftServer server, Caso caso) {
        TileEntityMekanism tile = tile(server, caso);
        boolean infuser = tile instanceof TileEntityMetallurgicInfuser;
        int inputSlot = infuser ? 1 : 0;
        int outputSlot = infuser ? 2 : 1;
        if (caso.args().containsKey("entrada")) {
            setItem(tile, inputSlot, caso.args().get("entrada"));
        }
        if (caso.args().containsKey("saida")) {
            setItem(tile, outputSlot, caso.args().get("saida"));
        }
        if (caso.args().containsKey("quimico") && tile instanceof TileEntityMetallurgicInfuser machine) {
            String[] parts = caso.args().get("quimico").split(":");
            var key = switch (parts[0]) {
                case "bio" -> ChemicalIds.BIO;
                case "fungi" -> ChemicalIds.FUNGI;
                case "carbon" -> ChemicalIds.CARBON;
                case "redstone" -> ChemicalIds.REDSTONE;
                case "gold" -> ChemicalIds.GOLD;
                default -> throw new IllegalArgumentException(parts[0]);
            };
            Holder<Chemical> chemical = server.overworld().registryAccess().getOrThrow(key);
            machine.infusionTank.setContents(ChemicalResource.of(chemical), Long.parseLong(parts[1]), null);
        }
        if (caso.args().containsKey("lava") && tile instanceof TileEntityHeatGenerator generator) {
            generator.lavaTank.setContents(FluidResource.of(Fluids.LAVA), Long.parseLong(caso.args().get("lava")), null);
        }
        if (caso.args().containsKey("energia")) {
            BasicEnergyContainer container = (BasicEnergyContainer) tile.getEnergyContainer();
            String value = caso.args().get("energia");
            Mekanism.logger.info("cena: {} capacidade {}", caso.name(), container.getCapacityAsLong());
            container.setEnergy(value.equals("cap") ? container.getCapacityAsLong() : Long.parseLong(value), null);
        }
    }

    /// O port ainda não tem as abas de configuração, transporte, cadeado, livro, energia, redstone e upgrades, nem a que guarda os
    /// slots de armadura: no original da cena elas ficam invisíveis, para a imagem mostrar o que o port tem. A aba de avisos fica.
    private static void hideUnimplementedTabs(Minecraft minecraft) {
        if (minecraft.gui.screen() instanceof mekanism.client.gui.GuiMekanism<?> screen) {
            for (var child : screen.children()) {
                if (child instanceof mekanism.client.gui.element.GuiElement element
                      && !(element instanceof mekanism.client.gui.element.tab.GuiWarningTab)
                      && element instanceof mekanism.client.gui.element.GuiSideHolder) {
                    element.visible = false;
                }
            }
        }
    }

    private static void open(Minecraft minecraft, Caso caso) {
        BlockPos pos = new BlockPos(caso.x(), Y, Z);
        BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(pos), Direction.NORTH, pos, false);
        minecraft.gameMode.useItemOn(minecraft.player, InteractionHand.MAIN_HAND, hit);
    }

    /// O cursor: `MouseHandler` guarda a posição em pixels da janela, e o jogo a escala para a GUI na hora de desenhar. Sem `hover`,
    /// o cursor vai para o canto, longe de tudo.
    private static void hover(Minecraft minecraft, Caso caso) {
        try {
            double scale = minecraft.getWindow().getGuiScale();
            double guiX = 2;
            double guiY = 2;
            if (caso.args().containsKey("hover") && minecraft.gui.screen() instanceof mekanism.client.gui.IGuiWrapper wrapper) {
                String[] xy = caso.args().get("hover").split(",");
                guiX = wrapper.getLeftPos() + Double.parseDouble(xy[0]) + 0.5;
                guiY = wrapper.getTopPos() + Double.parseDouble(xy[1]) + 0.5;
            }
            Field x = MouseHandler.class.getDeclaredField("xpos");
            Field y = MouseHandler.class.getDeclaredField("ypos");
            x.setAccessible(true);
            y.setAccessible(true);
            x.setDouble(minecraft.mouseHandler, guiX * scale);
            y.setDouble(minecraft.mouseHandler, guiY * scale);
        } catch (ReflectiveOperationException e) {
            Mekanism.logger.error("cena: sem cursor", e);
        }
    }
}
