package mekanism.common.tests.network;

import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.file.Files;
import java.lang.reflect.Method;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import mekanism.api.chemical.Chemical;
import mekanism.api.chemical.ChemicalResource;
import mekanism.api.chemical.ChemicalIds;
import mekanism.api.energy.IEnergyContainer;
import mekanism.common.component.containers.type.ContainerType;
import mekanism.common.util.EnergyUtils;
import mekanism.common.util.ItemAccessUtils;
import net.neoforged.neoforge.transfer.access.ItemAccess;
import net.neoforged.neoforge.transfer.energy.EnergyHandler;
import net.minecraft.world.item.ItemStack;
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
import net.minecraft.nbt.CompoundTag;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
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
/// GUI relativos à tela), `capturas` (quantos prints, um a cada 2 ticks) e `reabrir` (o tile é gravado e relido do NBT depois dos passos, e a tela abre antes do primeiro tick dele).
///
/// O servidor fica parado (`tick freeze`) enquanto o estado é montado e avança `passos` ticks com `tick step`, o que dá o mesmo
/// progresso nos dois jogos.
@EventBusSubscriber(modid = MekanismTests.MODID, value = Dist.CLIENT)
public class CenaTelas {

    private static final Path ATIVA = Path.of("cena-telas");
    private static final int Y = -60;
    private static final int Z = 3;
    private static final int OPEN_WAIT = 60;
    private static final int CLICK_GAP = 8;

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

    /// O rastro dos tooltips dos químicos (frente `tanques`, 7.ª rodada): `ChemicalStack.getTooltipLines` de cada químico do registro, no cliente (as unidades vêm da
    /// configuração local), sem e com os tooltips avançados. Cada linha é uma lista de trechos `[texto, cor]` (a cor que o estilo herdado dá, `null` sem cor), com os
    /// vizinhos de mesma cor juntos. Vai para `chemical_tooltips.jsonl`, na pasta do cliente.
    private static void chemicalTooltips(Minecraft minecraft) {
        StringBuilder out = new StringBuilder();
        var registry = minecraft.level.registryAccess().lookupOrThrow(mekanism.api.MekanismRegistries.Keys.CHEMICAL);
        var context = net.minecraft.world.item.Item.TooltipContext.of(minecraft.level, minecraft.player);
        registry.listElements().sorted(java.util.Comparator.comparing(holder -> holder.key().identifier().toString())).forEach(holder -> {
            mekanism.api.chemical.ChemicalStack stack = new mekanism.api.chemical.ChemicalStack(holder, 1_000);
            out.append("{\"id\":\"").append(holder.key().identifier()).append('"');
            for (var flag : List.of(net.minecraft.world.item.TooltipFlag.Default.NORMAL, net.minecraft.world.item.TooltipFlag.Default.ADVANCED)) {
                out.append(",\"").append(flag.isAdvanced() ? "advanced" : "normal").append("\":[");
                List<net.minecraft.network.chat.Component> lines = stack.getTooltipLines(context, minecraft.player, flag);
                for (int i = 0; i < lines.size(); i++) {
                    List<String[]> pieces = new ArrayList<>();
                    lines.get(i).visit((style, text) -> {
                        String color = style.getColor() == null ? null : String.format("#%06X", style.getColor().getValue());
                        if (!text.isEmpty()) {
                            if (!pieces.isEmpty() && java.util.Objects.equals(pieces.getLast()[1], color)) {
                                pieces.getLast()[0] += text;
                            } else {
                                pieces.add(new String[]{text, color});
                            }
                        }
                        return java.util.Optional.empty();
                    }, net.minecraft.network.chat.Style.EMPTY);
                    out.append(i == 0 ? "[" : ",[");
                    for (int j = 0; j < pieces.size(); j++) {
                        String color = pieces.get(j)[1];
                        out.append(j == 0 ? "[" : ",[").append(new com.google.gson.JsonPrimitive(pieces.get(j)[0])).append(',')
                              .append(color == null ? "null" : "\"" + color + "\"").append(']');
                    }
                    out.append(']');
                }
                out.append(']');
            }
            out.append("}\n");
        });
        try {
            Files.writeString(Path.of("chemical_tooltips.jsonl"), out.toString());
        } catch (IOException e) {
            Mekanism.logger.error("cena: sem o rastro dos tooltips", e);
        }
    }

    /// O rastro das abas criativas com os tanques cheios (frente `tanques`, 7.ª rodada): a aba do Mekanism e a Tools & Utilities como o cliente as monta
    /// (`CreativeModeTabs.tryRebuildTabContents`), um item por linha na ordem da aba, com o fluido ou o químico que a pilha guarda. Vai para `creative_tab_tanks.jsonl`.
    private static void creativeTabs(Minecraft minecraft) {
        StringBuilder out = new StringBuilder();
        net.minecraft.world.item.CreativeModeTabs.tryRebuildTabContents(minecraft.player.connection.enabledFeatures(), true, minecraft.level.registryAccess());
        for (var entry : BuiltInRegistries.CREATIVE_MODE_TAB.entrySet()) {
            String tab = entry.getKey().identifier().toString();
            if (!tab.equals("mekanism:mekanism") && !tab.equals("minecraft:tools_and_utilities")) {
                continue;
            }
            int index = 0;
            for (ItemStack stack : entry.getValue().getDisplayItems()) {
                out.append("{\"tab\":\"").append(tab).append("\",\"index\":").append(index++).append(",\"item\":\"").append(BuiltInRegistries.ITEM.getKey(stack.getItem())).append('"');
                var fluids = stack.get(mekanism.common.registries.MekanismDataComponents.ATTACHED_FLUIDS);
                if (fluids != null && fluids.hasNonEmptyContents()) {
                    var held = fluids.containers().getFirst();
                    out.append(",\"fluid\":\"").append(BuiltInRegistries.FLUID.getKey(held.resource().getFluid())).append("\",\"amount\":").append(held.amount());
                }
                var chemicals = stack.get(mekanism.common.registries.MekanismDataComponents.ATTACHED_CHEMICALS);
                if (chemicals != null && chemicals.hasNonEmptyContents()) {
                    var held = chemicals.containers().getFirst();
                    out.append(",\"chemical\":\"").append(held.resource().typeHolder().getRegisteredName()).append("\",\"amount\":").append(held.amount());
                }
                out.append("}\n");
            }
        }
        try {
            Files.writeString(Path.of("creative_tab_tanks.jsonl"), out.toString());
        } catch (IOException e) {
            Mekanism.logger.error("cena: sem o rastro das abas", e);
        }
    }

    private static BlockState block(String machine) {
        return switch (machine) {
            case "chamber" -> MekanismBlocks.ENRICHMENT_CHAMBER.defaultState();
            case "crusher" -> MekanismBlocks.CRUSHER.defaultState();
            case "smelter" -> MekanismBlocks.ENERGIZED_SMELTER.defaultState();
            case "infuser" -> MekanismBlocks.METALLURGIC_INFUSER.defaultState();
            case "generator" -> GeneratorsBlocks.HEAT_GENERATOR.defaultState();
            case "cubo" -> MekanismBlocks.BASIC_ENERGY_CUBE.defaultState();
            case "cubo_ultimate" -> MekanismBlocks.ULTIMATE_ENERGY_CUBE.defaultState();
            case "cubo_criativo" -> MekanismBlocks.CREATIVE_ENERGY_CUBE.defaultState();
            case "tanque" -> MekanismBlocks.BASIC_FLUID_TANK.defaultState();
            case "tanque_criativo" -> MekanismBlocks.CREATIVE_FLUID_TANK.defaultState();
            case "tanque_quimico" -> MekanismBlocks.BASIC_CHEMICAL_TANK.defaultState();
            case "tanque_quimico_criativo" -> MekanismBlocks.CREATIVE_CHEMICAL_TANK.defaultState();
            default -> throw new IllegalArgumentException(machine);
        };
    }

    private static final java.nio.file.Path SOM = java.nio.file.Path.of("/home/luka/.local/share/jogo-isolado/mekanism-ref/dois-sync");

    @SubscribeEvent
    public static void onTick(ClientTickEvent.Post event) {
        Minecraft minecraft = Minecraft.getInstance();
        MinecraftServer server = minecraft.getSingleplayerServer();
        if (minecraft.level == null || minecraft.player == null || server == null || !Files.exists(ATIVA)) {
            return;
        }
        // O vídeo com som (frente `tanques`, 5.ª rodada), como a `CenaDois`: o jogo abre mudo, e o volume mestre e o da categoria `ui` só sobem quando quem grava já
        // conferiu que o som do jogo está no sink de teste (`dois-sync/som-video-ok`); `som-video-mudo` (algum som saiu do sink de teste, ou a gravação acabou) o
        // devolve a 0 na hora.
        var master = minecraft.options.getSoundSourceOptionInstance(net.minecraft.sounds.SoundSource.MASTER);
        if (Files.exists(SOM.resolve("som-video-mudo"))) {
            if (master.get() > 0) {
                master.set(0.0);
            }
        } else if (Files.exists(SOM.resolve("som-video-ok")) && master.get() == 0) {
            for (var source : net.minecraft.sounds.SoundSource.values()) {
                minecraft.options.getSoundSourceOptionInstance(source).set(source == net.minecraft.sounds.SoundSource.UI || source == net.minecraft.sounds.SoundSource.MASTER ? 1.0 : 0.0);
            }
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
                // O Horizon não mostra o retorno dos comandos no chat; o original mostraria `[Server: ...]` no canto do print.
                minecraft.options.chatVisibility().set(net.minecraft.world.entity.player.ChatVisiblity.HIDDEN);
                chemicalTooltips(minecraft);
                creativeTabs(minecraft);
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
        // `energia_depois=N`: a energia chega depois dos passos, fora do tick da máquina (um cabo que enche o contêiner depois de ela ter rodado): o erro
        // de energia da máquina pausada fica aceso com a barra cheia. Com o servidor parado nenhum tick o limpa antes de a tela abrir.
        if (caseTick == 20 + steps + 9 && caso.args().containsKey("energia_depois")) {
            server.execute(() -> {
                BasicEnergyContainer container = (BasicEnergyContainer) tile(server, caso).getEnergyContainer();
                container.setEnergy(Long.parseLong(caso.args().get("energia_depois")), null);
            });
        }
        // `reabrir=1`: o mundo foi salvo e reaberto com a máquina como está (o tile é gravado e relido do NBT, sem nenhum tick depois), e a tela abre antes do primeiro
        // tick do tile novo. O servidor segue parado, então nada o tica até o print.
        if (caseTick == 20 + steps + 9 && caso.args().containsKey("reabrir")) {
            server.execute(() -> reopen(server, caso));
        }
        // O `tick step` roda a 20 ticks por segundo, o mesmo ritmo do cliente: espera os passos e uma folga.
        int base = 25 + steps + 20;
        if (caseTick >= base) {
            orderClientUpgrades(minecraft);
        }
        // Um clique por vez, `CLICK_GAP` ticks entre eles: a janela de upgrades e a lista dela só recebem os dados do servidor depois do primeiro clique,
        // e dois cliques no mesmo tick pegavam a lista antes da ordem refeita (e, às vezes, vazia).
        int clickCount = caso.args().containsKey("clicar") ? caso.args().get("clicar").split(";").length : 0;
        int first = base + OPEN_WAIT;
        int last = first + CLICK_GAP * Math.max(0, clickCount - 1);
        if (caseTick == base) {
            report(minecraft, caso, "ao abrir");
            open(minecraft, caso);
        } else if (caseTick >= first && caseTick <= last && (caseTick - first) % CLICK_GAP == 0) {
            if (caseTick == first) {
                hideUnimplementedTabs(minecraft, caso);
            }
            click(minecraft, caso, "clicar", (caseTick - first) / CLICK_GAP);
            if (caseTick == last) {
                type(minecraft, caso);
                report(minecraft, caso, "depois dos cliques");
                hover(minecraft, caso);
                shotsTaken = 0;
            }
        } else if (cycle > 0 && caseTick > last + 10 && shotsTaken < captures && (caseTick - base) % 6 == 0) {
            // Um passo do servidor por quadro: as animações do atlas só andam com o jogo rodando, e a fase de cada print é conhecida.
            server.execute(() -> run(server, "tick step 1"));
        } else if (cycle > 0 && caseTick > last + 10 && shotsTaken < captures && (caseTick - base) % 6 == 3) {
            shot(minecraft, "oficial_" + caso.name() + "_e" + caso.scale() + "_" + shotsTaken);
            shotsTaken++;
        } else if (cycle == 0 && caseTick > last + 10 && shotsTaken < captures && (caseTick - base) % 2 == 0) {
            String suffix = captures > 1 ? "_" + shotsTaken : "";
            shot(minecraft, "oficial_" + caso.name() + "_e" + caso.scale() + suffix);
            shotsTaken++;
        } else if (caseTick == last + 14 + captures * (cycle > 0 ? 6 : 2)) {
            int undo = caso.args().containsKey("desfazer") ? caso.args().get("desfazer").split(";").length : 0;
            for (int i = 0; i < undo; i++) {
                click(minecraft, caso, "desfazer", i);
            }
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
        //O bioma decide a temperatura do ar que a aba de calor mostra: planície nos dois jogos
        run(server, "fillbiome " + (x - 6) + " -61 -6 " + (x + 6) + " -50 10 minecraft:plains");
    }

    private static void build(MinecraftServer server, Caso caso) {
        box(server, caso);
        // O controle da luz (`luz=N`): uma fonte vanilla de nível N ao lado da máquina, sem nenhum bloco do Mekanism emitindo, para medir como cada jogo
        // desenha o mundo iluminado atrás da tela.
        if (caso.args().containsKey("luz")) {
            run(server, "setblock " + (caso.x() + 2) + " " + Y + " " + Z + " minecraft:light[level=" + caso.args().get("luz") + "]");
        }
        ServerLevel level = server.overworld();
        level.setBlockAndUpdate(new BlockPos(caso.x(), Y, Z), Blocks.AIR.defaultBlockState());
        level.setBlockAndUpdate(new BlockPos(caso.x(), Y, Z), block(caso.machine()));
        // Como um jogador coloca a máquina: `applyImplicitComponents` recalcula os upgrades, e com eles a capacidade de energia.
        tile(server, caso).applyComponents(net.minecraft.core.component.DataComponentMap.EMPTY,
              net.minecraft.core.component.DataComponentPatch.EMPTY);
        // `BlockMekanism.setPlacedBy`: quem põe a máquina é o dono (o port a põe pelo jogador, `place`), e a aba de segurança mostra o dono e o modo dele.
        var owner = server.getPlayerList().getPlayers().getFirst();
        tile(server, caso).setOwnerUUID(owner.getUUID(), null);
    }

    /// A configuração de lados e o ejetor (as janelas de lados e de transporte): `lados=items:front=output,top=none;energy:back=input` (um tipo, seguido de
    /// lado=DataType, com os nomes serializados), `ejetar=items,chemicals`, `strict=1`, `cor_saida=red` e `cor_entrada=front:blue,top:yellow`. Como a máquina da cena
    /// não é posta por um item, cada lado que a cena quer fica dito aqui. `sideChanged` avisa o cliente (o pacote de atualização do tile).
    private static void configuracao(TileEntityMekanism tile, Caso caso) {
        if (!(tile instanceof mekanism.common.tile.prefab.TileEntityConfigurableMachine machine)) {
            return;
        }
        var config = machine.getConfig();
        if (caso.args().containsKey("lados")) {
            for (String group : caso.args().get("lados").split(";")) {
                String[] parts = group.split(":", 2);
                var type = config.getTransmissions().stream().filter(t -> t.getSerializedName().equals(parts[0])).findFirst().orElseThrow();
                var info = config.getConfig(type);
                for (String entry : parts[1].split(",")) {
                    String[] pair = entry.split("=");
                    var side = mekanism.api.RelativeSide.valueOf(pair[0].toUpperCase());
                    var dataType = java.util.Arrays.stream(mekanism.common.tile.component.config.DataType.values()).filter(d -> d.getSerializedName().equals(pair[1])).findFirst().orElseThrow();
                    info.setDataType(dataType, side);
                    config.sideChanged(type, side);
                }
            }
        }
        if (caso.args().containsKey("ejetar")) {
            for (String name : caso.args().get("ejetar").split(",")) {
                config.getTransmissions().stream().filter(t -> t.getSerializedName().equals(name)).forEach(t -> config.getConfig(t).setEjecting(true));
            }
        }
        var ejector = machine.getEjector();
        if (caso.args().containsKey("strict")) {
            ejector.setStrictInput(true);
        }
        if (caso.args().containsKey("cor_saida")) {
            ejector.setOutputColor(color(caso.args().get("cor_saida")));
        }
        if (caso.args().containsKey("cor_entrada")) {
            for (String entry : caso.args().get("cor_entrada").split(",")) {
                String[] pair = entry.split(":");
                ejector.setInputColor(mekanism.api.RelativeSide.valueOf(pair[0].toUpperCase()), color(pair[1]));
            }
        }
        machine.markForSave();
    }

    private static mekanism.api.text.EnumColor color(String name) {
        return mekanism.api.text.EnumColor.VALUES.stream().filter(c -> c.getSerializedName().equals(name)).findFirst().orElseThrow();
    }

    /// O que o jogo faz ao descarregar e carregar o chunk: grava o tile e cria outro do NBT (`BlockEntity.loadStatic`, que chama o `loadAdditional`), como o
    /// `ConstantUsageTraceTest.reload`. O tile novo ainda não tickou.
    private static void reopen(MinecraftServer server, Caso caso) {
        ServerLevel level = server.overworld();
        BlockPos pos = new BlockPos(caso.x(), Y, Z);
        TileEntityMekanism old = tile(server, caso);
        CompoundTag tag = old.saveWithFullMetadata(level.registryAccess());
        BlockEntity loaded = BlockEntity.loadStatic(pos, level.getBlockState(pos), tag, level.registryAccess());
        if (loaded == null || loaded == old) {
            throw new IllegalStateException("loadStatic gave " + loaded);
        }
        LevelChunk chunk = level.getChunkAt(pos);
        old.onChunkUnloaded();
        chunk.removeBlockEntity(pos);
        chunk.addAndRegisterBlockEntity(loaded);
        Mekanism.logger.info("cena: {} reaberto {}", caso.name(), tag);
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

    /// `ranhura=ITEM:ENERGIA` põe no slot de energia que enche a máquina (o de descarga do cubo, o de energia do gerador e das máquinas) um item com `ENERGIA` no componente
    /// `mekanism:energy`, e `carga=ITEM:ENERGIA` no slot de carga do cubo, como o item de um jogador (o handler de energia do próprio item).
    private static void energySlot(TileEntityMekanism tile, String spec, boolean charge) {
        String[] parts = spec.split(":");
        String id = parts[0] + ":" + parts[1];
        long energy = Long.parseLong(parts[2]);
        var item = BuiltInRegistries.ITEM.getValue(Identifier.parse(id));
        ItemAccess access = ItemAccessUtils.sideEffectFreeAccess(ItemResource.of(item));
        EnergyHandler handler = ContainerType.ENERGY.getCapOrUnexposed(access);
        ItemStack stack = new ItemStack(item, 1);
        if (handler != null && energy != 0) {
            IEnergyContainer container = EnergyUtils.getEnergyContainer(handler);
            container.setEnergy(energy, null);
            stack = ItemAccessUtils.asStack(access);
            stack.setCount(1);
        }
        int slot;
        if (tile instanceof mekanism.common.tile.TileEntityEnergyCube) {
            slot = charge ? 1 : 0;
        } else if (tile instanceof TileEntityHeatGenerator) {
            slot = 1;
        } else {
            slot = tile instanceof TileEntityMetallurgicInfuser ? 3 : 2;
        }
        tile.getInventorySlots().get(slot).setContents(ItemResource.of(stack), 1, null);
    }

    private static void state(MinecraftServer server, Caso caso) {
        TileEntityMekanism tile = tile(server, caso);
        if (caso.name().contains("_livro_")) {
            // O inventário de cada caso do livro começa vazio e leva o que `itens=slot:item:quantidade,...` diz (o `canCraft` e a colocação o leem).
            run(server, "clear @a");
            if (caso.args().containsKey("itens")) {
                for (String item : caso.args().get("itens").split(",")) {
                    giveItem(server, item);
                }
            }
        }
        if (!caso.name().contains("_livro_") && caso.args().containsKey("itens")) {
            // Fora do livro (frente `tanques`, 7.ª rodada): o item no inventário para o tooltip dele, com o cursor no slot.
            run(server, "clear @a");
            for (String item : caso.args().get("itens").split(",")) {
                giveItem(server, item);
            }
        }
        if (caso.args().containsKey("receitas")) {
            // O livro de receitas com tudo desbloqueado (`/recipe give @a *`, o mesmo comando no port).
            run(server, "recipe give @a *");
        } else if (caso.name().contains("_livro_")) {
            // Os outros casos do livro, com ele vazio, como o port (`cena_telas.monta_port`).
            run(server, "recipe take @a *");
        }
        if (caso.args().containsKey("ranhura")) {
            energySlot(tile, caso.args().get("ranhura"), false);
        }
        if (caso.args().containsKey("carga")) {
            energySlot(tile, caso.args().get("carga"), true);
        }
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
        // Os tanques (frente `tanques`, 5.ª rodada): `fluido=water:16000` (ou `lava`, ou `cap` no lugar da quantidade) no Fluid Tank, `modo=fill|empty` (o modo de edição,
        // pelo `nextMode` da aba), `quimico=hydrogen:5000` no Chemical Tank e `descarte=dumping_excess|dumping`.
        if (caso.args().containsKey("fluido") && tile instanceof mekanism.common.tile.TileEntityFluidTank tank) {
            String[] parts = caso.args().get("fluido").split(":");
            FluidResource fluid = FluidResource.of(parts[0].equals("lava") ? Fluids.LAVA : Fluids.WATER);
            tank.fluidTank.setContents(fluid, parts[1].equals("cap") ? tank.fluidTank.capacityAsLong(fluid) : Long.parseLong(parts[1]), null);
        }
        if (caso.args().containsKey("modo") && tile instanceof mekanism.common.tile.TileEntityFluidTank tank) {
            for (int i = caso.args().get("modo").equals("fill") ? 1 : 2; i > 0; i--) {
                tank.nextMode();
            }
        }
        if (caso.args().containsKey("quimico") && tile instanceof mekanism.common.tile.TileEntityChemicalTank tank) {
            String[] parts = caso.args().get("quimico").split(":");
            Holder<Chemical> chemical = server.overworld().registryAccess().getOrThrow(
                  net.minecraft.resources.ResourceKey.create(mekanism.api.MekanismRegistries.Keys.CHEMICAL, Mekanism.rl(parts[0])));
            ChemicalResource resource = ChemicalResource.of(chemical);
            tank.getChemicalTank().setContents(resource, parts[1].equals("cap") ? tank.getChemicalTank().capacityAsLong(resource) : Long.parseLong(parts[1]), null);
        }
        if (caso.args().containsKey("descarte") && tile instanceof mekanism.common.tile.TileEntityChemicalTank tank) {
            for (int i = caso.args().get("descarte").equals("dumping_excess") ? 1 : 2; i > 0; i--) {
                tank.nextMode(0);
            }
        }
        if (caso.args().containsKey("lava") && tile instanceof TileEntityHeatGenerator generator) {
            generator.lavaTank.setContents(FluidResource.of(Fluids.LAVA), Long.parseLong(caso.args().get("lava")), null);
        }
        upgrades(server, tile, caso);
        configuracao(tile, caso);
        if (caso.args().containsKey("nome")) {
            // `nome=` é um componente em JSON, sem espaços: o texto, a cor e o negrito que o título da tela mostra.
            tile.setCustomName(net.minecraft.network.chat.ComponentSerialization.CODEC
                  .parse(com.mojang.serialization.JsonOps.INSTANCE, com.google.gson.JsonParser.parseString(caso.args().get("nome"))).getOrThrow());
        }
        if (caso.args().containsKey("energia")) {
            BasicEnergyContainer container = (BasicEnergyContainer) tile.getEnergyContainer();
            String value = caso.args().get("energia");
            Mekanism.logger.info("cena: {} capacidade {}", caso.name(), container.getCapacityAsLong());
            container.setEnergy(value.equals("cap") ? container.getCapacityAsLong() : Long.parseLong(value), null);
        }
    }

    private static Holder<mekanism.api.upgrade.Upgrade> upgrade(MinecraftServer server, String name) {
        return server.overworld().registryAccess().getOrThrow(
              net.minecraft.resources.ResourceKey.create(mekanism.api.MekanismRegistries.Keys.UPGRADES, Mekanism.rl(name)));
    }

    private static net.minecraft.world.item.ItemStack upgradeItem(MinecraftServer server, String spec) {
        String[] parts = spec.split(":");
        return mekanism.api.upgrade.IUpgradeHelper.INSTANCE.asStack(upgrade(server, parts[0]), Integer.parseInt(parts[1]));
    }

    /// `upgrades=speed:3,energy:2` instala, `upgradeentrada=speed:5` e `upgradesaida=speed:7` enchem os dois slots do componente.
    private static void upgrades(MinecraftServer server, TileEntityMekanism tile, Caso caso) {
        var component = tile.getComponent();
        if (component == null) {
            return;
        }
        if (caso.args().containsKey("upgrades")) {
            for (String spec : caso.args().get("upgrades").split(",")) {
                String[] parts = spec.split(":");
                component.addUpgrades(server.overworld().registryAccess(), upgrade(server, parts[0]), Integer.parseInt(parts[1]));
            }
        }
        orderUpgrades(server.overworld().registryAccess(), component);
        if (caso.args().containsKey("upgradeentrada")) {
            var stack = upgradeItem(server, caso.args().get("upgradeentrada"));
            component.getUpgradeSlot().setContents(ItemResource.of(stack), stack.count(), null);
        }
        if (caso.args().containsKey("upgradesaida")) {
            var stack = upgradeItem(server, caso.args().get("upgradesaida"));
            component.getUpgradeOutputSlot().setContents(ItemResource.of(stack), stack.count(), null);
        }
    }

    /// A lista da janela de upgrades vem de `getInstalledTypes()`, as chaves de um `Object2IntOpenHashMap<Holder<Upgrade>>`. O `hashCode` de um
    /// `Holder.Reference` é o de identidade, que muda com o que o JVM já sorteou antes (medido: a mesma cena, sozinha ou depois de outros
    /// casos, dá ordens diferentes), então o original não tem ordem a copiar. O mapa vira um de ordem de inserção na ordem do registro, a
    /// mesma em que a janela lista os tipos suportados e o port lista os instalados, para os prints se compararem linha a linha.
    private static void orderUpgrades(net.minecraft.core.RegistryAccess registries, mekanism.common.tile.component.TileComponentUpgrade component) {
        try {
            Field field = mekanism.common.tile.component.TileComponentUpgrade.class.getDeclaredField("upgrades");
            field.setAccessible(true);
            @SuppressWarnings("unchecked")
            var current = (it.unimi.dsi.fastutil.objects.Object2IntMap<Holder<mekanism.api.upgrade.Upgrade>>) field.get(component);
            var ordered = new it.unimi.dsi.fastutil.objects.Object2IntLinkedOpenHashMap<Holder<mekanism.api.upgrade.Upgrade>>();
            registries.lookupOrThrow(mekanism.api.MekanismRegistries.Keys.UPGRADES).listElements().forEach(holder -> {
                if (current.containsKey(holder)) {
                    ordered.put(holder, current.getInt(holder));
                }
            });
            if (!new ArrayList<>(current.keySet()).equals(new ArrayList<>(ordered.keySet()))) {
                field.set(component, ordered);
            }
        } catch (ReflectiveOperationException e) {
            Mekanism.logger.error("cena: sem ordem dos upgrades", e);
        }
    }

    /// O cliente tem a sua própria cópia do tile, e a sincronização do mapa (`SyncableStreamCodec.upgradeMap`) o troca por um
    /// `Object2IntOpenHashMap` novo: a ordem se refaz antes de cada clique e de cada quadro.
    private static void orderClientUpgrades(Minecraft minecraft) {
        if (minecraft.level != null && minecraft.gui.screen() instanceof mekanism.client.gui.GuiMekanismTile<?, ?> screen
              && screen.getMenu().getTileEntity().getComponent() != null) {
            orderUpgrades(minecraft.level.registryAccess(), screen.getMenu().getTileEntity().getComponent());
        }
    }

    /// `clicar=x,y;x,y` (e `desfazer=...`, o que se clica ao fim do caso para devolver o estado de cliente que os cliques mudaram): cliques do botão esquerdo, em pixels de GUI relativos ao canto da tela, pelo `mouseClicked` da tela como o mouse faria.
    private static void click(Minecraft minecraft, Caso caso, String key, int index) {
        if (!caso.args().containsKey(key) || !(minecraft.gui.screen() instanceof mekanism.client.gui.IGuiWrapper wrapper)) {
            return;
        }
        var screen = minecraft.gui.screen();
        String token = caso.args().get(key).split(";")[index];
        // Entre os cliques: `t=texto` digita (o `_` vale um espaço), `k=enter` aperta o Enter e `i=slot:item:quantidade` põe um item no
        // inventário do jogador (`item replace`), como o `itens=` faz antes de a tela abrir.
        if (token.startsWith("t=")) {
            token.substring(2).replace('_', ' ').codePoints().forEach(cp -> screen.charTyped(new net.minecraft.client.input.CharacterEvent(cp)));
            return;
        } else if (token.startsWith("k=")) {
            // `k=tecla` ou `k=mods-tecla`, com `s` (shift) e `c` (ctrl) em `mods`: `k=enter`, `k=left`, `k=s-home`, `k=c-backspace`, `k=cs-right`.
            String name = token.substring(2);
            int modifiers = 0;
            int dash = name.indexOf('-');
            if (dash > 0) {
                String mods = name.substring(0, dash);
                name = name.substring(dash + 1);
                modifiers = (mods.contains("s") ? com.mojang.blaze3d.platform.InputConstants.MOD_SHIFT : 0)
                            | (mods.contains("c") ? com.mojang.blaze3d.platform.InputConstants.MOD_CONTROL : 0);
            }
            // A tecla (`key`, o código físico) e o `keycode` (o do SDL, que `EditBox.keyPressed` lê por `shortcutKey`).
            int[] code = switch (name) {
                case "left" -> new int[] {com.mojang.blaze3d.platform.InputConstants.KEY_LEFT, 1073741904};
                case "right" -> new int[] {com.mojang.blaze3d.platform.InputConstants.KEY_RIGHT, 1073741903};
                case "home" -> new int[] {com.mojang.blaze3d.platform.InputConstants.KEY_HOME, 1073741898};
                case "end" -> new int[] {com.mojang.blaze3d.platform.InputConstants.KEY_END, 1073741901};
                case "backspace" -> new int[] {com.mojang.blaze3d.platform.InputConstants.KEY_BACKSPACE, 8};
                case "delete" -> new int[] {com.mojang.blaze3d.platform.InputConstants.KEY_DELETE, 127};
                case "a" -> new int[] {com.mojang.blaze3d.platform.InputConstants.KEY_A, com.mojang.blaze3d.platform.InputConstants.KEYCODE_A};
                default -> new int[] {com.mojang.blaze3d.platform.InputConstants.KEY_RETURN, 13};
            };
            screen.keyPressed(new net.minecraft.client.input.KeyEvent(code[0], code[1], modifiers));
            return;
        } else if (token.startsWith("d=")) {
            // `d=x1,y1,x2,y2`: o botão esquerdo apertado no primeiro ponto, arrastado até o segundo e solto lá.
            String[] p = token.substring(2).split(",");
            double x1 = wrapper.getLeftPos() + Double.parseDouble(p[0]) + 0.5, y1 = wrapper.getTopPos() + Double.parseDouble(p[1]) + 0.5;
            double x2 = wrapper.getLeftPos() + Double.parseDouble(p[2]) + 0.5, y2 = wrapper.getTopPos() + Double.parseDouble(p[3]) + 0.5;
            var left = new net.minecraft.client.input.MouseButtonInfo(com.mojang.blaze3d.platform.InputConstants.MOUSE_BUTTON_LEFT, 0);
            screen.mouseClicked(new net.minecraft.client.input.MouseButtonEvent(x1, y1, left), false);
            screen.mouseDragged(new net.minecraft.client.input.MouseButtonEvent(x2, y2, left), x2 - x1, y2 - y1);
            screen.mouseReleased(new net.minecraft.client.input.MouseButtonEvent(x2, y2, left));
            return;
        } else if (token.startsWith("i=")) {
            MinecraftServer server = minecraft.getSingleplayerServer();
            String item = token.substring(2);
            server.execute(() -> giveItem(server, item));
            return;
        }
        // `x,y`, `x,y,r` (o botão direito) ou `x,y,s` (o esquerdo com o shift).
        String[] xy = token.split(",");
        double x = wrapper.getLeftPos() + Double.parseDouble(xy[0]) + 0.5;
        double y = wrapper.getTopPos() + Double.parseDouble(xy[1]) + 0.5;
        orderClientUpgrades(minecraft);
        boolean right = xy.length > 2 && xy[2].equals("r");
        boolean shift = xy.length > 2 && xy[2].equals("s");
        var info = new net.minecraft.client.input.MouseButtonInfo(right ? com.mojang.blaze3d.platform.InputConstants.MOUSE_BUTTON_RIGHT
                                                                        : com.mojang.blaze3d.platform.InputConstants.MOUSE_BUTTON_LEFT, shift ? 1 : 0);
        screen.mouseClicked(new net.minecraft.client.input.MouseButtonEvent(x, y, info), false);
        screen.mouseReleased(new net.minecraft.client.input.MouseButtonEvent(x, y, info));
    }

    /// `slot:item:quantidade` (ou `slot:ns:item:quantidade`) no inventário de quem joga.
    private static void giveItem(MinecraftServer server, String spec) {
        String[] parts = spec.split(":");
        String id = parts.length == 4 ? parts[1] + ":" + parts[2] : "minecraft:" + parts[1];
        run(server, "item replace entity @a container." + parts[0] + " with " + id + " " + parts[parts.length - 1]);
    }

    /// `digitar=texto`: depois dos cliques, cada caractere pelo `charTyped` da tela, como o teclado faria (a caixa de busca do livro, focada por um clique).
    private static void type(Minecraft minecraft, Caso caso) {
        var screen = minecraft.gui.screen();
        if (!caso.args().containsKey("digitar") || screen == null) {
            return;
        }
        caso.args().get("digitar").codePoints().forEach(cp -> screen.charTyped(new net.minecraft.client.input.CharacterEvent(cp)));
    }

    /// O port tem as abas de livro, segurança, redstone, configuração e transporte (o livro desde 2026-11-15, frente `livro-receitas`): só as abas que ainda lhe faltam ficam invisíveis no original da cena, para a
    /// imagem mostrar o que o port tem. A aba de avisos, a de upgrades e a de xp ficam; a de xp vai para o log (`cena: ... xp_tab`) com a
    /// posição e a visibilidade que o original dá. O suporte da barra de armadura do Heat Generator (`GuiSideHolder.create(-26, 6, 98)`, `isArmorHolder`)
    /// também fica visível desde 2026-11-07, quando o port o desenhou: antes ficava oculto, e o comparador só via os slots.
    private static void hideUnimplementedTabs(Minecraft minecraft, Caso caso) {
        if (minecraft.gui.screen() instanceof mekanism.client.gui.GuiMekanism<?> screen) {
            // `fora=1`: o que `GuiMekanism.hasClickedOutside` responde numa grade ao redor da tela, antes de esconder qualquer aba (`completo`), com as abas que o
            // port não tem escondidas (`oculto`) e, além delas, sem a aba de upgrades, que a janela de upgrades da lib responde (`sem_upgrades`).
            boolean outside = caso.args().containsKey("fora");
            if (outside) {
                recordOutside(screen, caso, "completo");
            }
            //Tudo o que o original desenha fora do retângulo da tela (176 por 166), para saber o que o comparador não vê
            for (var child : screen.children()) {
                if (child instanceof mekanism.client.gui.element.GuiElement element) {
                    int x = element.getX() - screen.getLeftPos();
                    int y = element.getY() - screen.getTopPos();
                    if (x < 0 || y < 0 || x + element.getWidth() > 176 || y + element.getHeight() > 166) {
                        Mekanism.logger.info("cena: {} fora {} x {} y {} largura {} altura {} visivel {}", caso.name(), element.getClass().getSimpleName(), x, y,
                              element.getWidth(), element.getHeight(), element.visible);
                    }
                }
            }
            for (var child : screen.children()) {
                if (child instanceof mekanism.client.gui.element.tab.GuiXpContainmentTab tab) {
                    Mekanism.logger.info("cena: {} xp_tab visible {} x {} y {} largura {} altura {}", caso.name(), tab.visible,
                          tab.getX() - screen.getLeftPos(), tab.getY() - screen.getTopPos(), tab.getWidth(), tab.getHeight());
                }
            }
            for (var child : screen.children()) {
                if (child instanceof mekanism.client.gui.element.GuiElement element
                      && !(element instanceof mekanism.client.gui.element.tab.GuiWarningTab)
                      && !(element instanceof mekanism.client.gui.element.tab.window.GuiUpgradeWindowTab)
                      && !(element instanceof mekanism.client.gui.element.tab.GuiXpContainmentTab)
                      && !(element instanceof mekanism.client.gui.element.tab.window.GuiRecipeBookWindowTab)
                      && !(element instanceof mekanism.client.gui.element.tab.GuiSecurityTab)
                      && !(element instanceof mekanism.client.gui.element.tab.GuiRedstoneControlTab)
                      && !(element instanceof mekanism.client.gui.element.tab.window.GuiSideConfigurationTab)
                      && !(element instanceof mekanism.client.gui.element.tab.window.GuiTransporterConfigTab)
                      && !(element instanceof mekanism.client.gui.element.tab.GuiContainerEditModeTab)
                      && !isArmorHolder(screen, element)
                      && element instanceof mekanism.client.gui.element.GuiSideHolder) {
                    element.visible = false;
                }
            }
            if (outside) {
                recordOutside(screen, caso, "oculto");
                for (var child : screen.children()) {
                    if (child instanceof mekanism.client.gui.element.tab.window.GuiUpgradeWindowTab tab) {
                        tab.visible = false;
                    }
                }
                recordOutside(screen, caso, "sem_upgrades");
            }
        }
    }

    private static final int X0 = -32;
    private static final int X1 = 240;
    private static final int Y0 = -6;
    private static final int Y1 = 176;

    /// A grade de `hasClickedOutside(x, y, left, top)` em pixels de GUI inteiros relativos ao canto da tela, de `X0` a `X1` e de `Y0` a `Y1` (sem o fim). Cada linha
    /// da grade é a lista de intervalos `[a, b)` de x em que a resposta é falsa, ou seja, em que o clique NÃO é "fora" (o que está dentro da imagem e o que está
    /// sobre um elemento ativo e visível). Vai em `fora_rastro.jsonl` na pasta do jogo, com os elementos que o original tem fora da imagem.
    private static void recordOutside(mekanism.client.gui.GuiMekanism<?> screen, Caso caso, String mode) {
        try {
            Method method = mekanism.client.gui.GuiMekanism.class.getDeclaredMethod("hasClickedOutside", double.class, double.class, int.class, int.class);
            method.setAccessible(true);
            int left = screen.getLeftPos();
            int top = screen.getTopPos();
            StringBuilder out = new StringBuilder();
            out.append("{\"case\":\"").append(caso.name()).append("\",\"mode\":\"").append(mode).append("\",\"x0\":").append(X0).append(",\"y0\":").append(Y0)
                  .append(",\"x1\":").append(X1).append(",\"y1\":").append(Y1).append(",\"elements\":[");
            boolean first = true;
            for (var child : screen.children()) {
                if (child instanceof mekanism.client.gui.element.GuiElement element) {
                    int x = element.getX() - left;
                    int y = element.getY() - top;
                    if (x < 0 || y < 0 || x + element.getWidth() > 176 || y + element.getHeight() > 166) {
                        out.append(first ? "" : ",").append("{\"class\":\"").append(element.getClass().getSimpleName()).append("\",\"x\":").append(x).append(",\"y\":")
                              .append(y).append(",\"w\":").append(element.getWidth()).append(",\"h\":").append(element.getHeight()).append(",\"visible\":")
                              .append(element.visible).append(",\"active\":").append(element.active).append("}");
                        first = false;
                    }
                }
            }
            out.append("],\"rows\":[");
            for (int y = Y0; y < Y1; y++) {
                out.append(y == Y0 ? "[" : ",[");
                int start = Integer.MIN_VALUE;
                boolean firstRun = true;
                for (int x = X0; x <= X1; x++) {
                    boolean over = x < X1 && !(boolean) method.invoke(screen, (double) (left + x), (double) (top + y), left, top);
                    if (over && start == Integer.MIN_VALUE) {
                        start = x;
                    } else if (!over && start != Integer.MIN_VALUE) {
                        out.append(firstRun ? "" : ",").append("[").append(start).append(",").append(x).append("]");
                        firstRun = false;
                        start = Integer.MIN_VALUE;
                    }
                }
                out.append("]");
            }
            out.append("]}\n");
            Files.writeString(Path.of("fora_rastro.jsonl"), out.toString(), StandardOpenOption.CREATE, StandardOpenOption.APPEND);
        } catch (ReflectiveOperationException | IOException e) {
            Mekanism.logger.error("cena: sem rastro de fora", e);
        }
    }

    /// O suporte da barra de armadura do Heat Generator (`GuiSideHolder.create(this, -26, 6, 98, ...)`): o único suporte que o port desenha nessa tela. Os da
    /// segurança e da redstone (também `GuiSideHolder`, à direita) seguem ocultos: o port não tem essas abas.
    private static boolean isArmorHolder(mekanism.client.gui.GuiMekanism<?> screen, mekanism.client.gui.element.GuiElement element) {
        // O Energy Cube tem o suporte da armadura à direita (`GuiSideHolder.create(this, imageWidth, 36, 98, false, true, ...)`), que o port desenha desde 2026-11-11.
        if (screen instanceof mekanism.client.gui.GuiEnergyCube) {
            return element.getX() - screen.getLeftPos() == 176 && element.getY() - screen.getTopPos() == 36;
        }
        // Os dois tanques têm o suporte do `GuiSideHolder.armorHolder` (-26, 62, 98), que o port desenha desde 2026-11-21.
        if (screen instanceof mekanism.client.gui.GuiFluidTank || screen instanceof mekanism.client.gui.GuiChemicalTank) {
            return element.getX() - screen.getLeftPos() == -26 && element.getY() - screen.getTopPos() == 62;
        }
        return screen instanceof mekanism.generators.client.gui.GuiHeatGenerator && element.getX() - screen.getLeftPos() == -26
              && element.getY() - screen.getTopPos() == 6;
    }

    /// O que o servidor tem do gerador (energia, lava, calor, produção) e do tanque do Infuser, no `cena:` do log, para comparar com o
    /// `MEKTRACE` do port. O servidor parado não muda enquanto o print sai.
    private static void report(Minecraft minecraft, Caso caso, String when) {
        var server = minecraft.getSingleplayerServer();
        if (server == null) {
            return;
        }
        server.execute(() -> {
            TileEntityMekanism tile = tile(server, caso);
            if (tile instanceof TileEntityHeatGenerator generator) {
                Mekanism.logger.info("cena: {} {} energia {} lava {} calor {} producao {}", caso.name(), when,
                      ((BasicEnergyContainer) generator.getEnergyContainer()).getAmountAsLong(), generator.lavaTank.amountAsLong(), generator.getTemperature(),
                      generator.getProductionRate());
            } else if (tile instanceof TileEntityMetallurgicInfuser machine) {
                Mekanism.logger.info("cena: {} {} tanque {}", caso.name(), when, machine.infusionTank.amountAsLong());
            }
        });
    }

    private static void open(Minecraft minecraft, Caso caso) {
        // `semtela`: o print do mundo sem tela aberta, para separar o que o jogo desenha no mundo do que a tela por cima faz com ele.
        if (caso.args().containsKey("semtela")) {
            return;
        }
        BlockPos pos = new BlockPos(caso.x(), Y, Z);
        BlockHitResult hit = new BlockHitResult(Vec3.atCenterOf(pos), Direction.NORTH, pos, false);
        var result = minecraft.gameMode.useItemOn(minecraft.player, InteractionHand.MAIN_HAND, hit);
        Mekanism.logger.info("cena: {} abrir {}", caso.name(), result);
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
