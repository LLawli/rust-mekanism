package mekanism.common.tests.network;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import mekanism.api.heat.HeatAPI;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.generators.common.registries.GeneratorsBlocks;
import mekanism.generators.common.tile.TileEntityHeatGenerator;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.commands.FillBiomeCommand;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.event.tick.ServerTickEvent;
import net.neoforged.neoforge.transfer.fluid.FluidResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Grava a temperatura do ar que o Heat Generator lê (`CachedAmbientTemperature` -> `HeatAPI.getAmbientTemp(level, pos)` ->
/// `Biome.getTemperature(pos, seaLevel)`, com o ajuste por altura acima de `seaLevel + 17`).
///
/// - `heat_ambient_table.jsonl`: sem tile nenhum. Para cada bioma, posição x/z, altura e nível do mar, a temperatura do bioma (o `float`
///   em bits) e a temperatura do ar (`double`); mais o nível do mar de cada dimensão e o `getAmbientTemp(level, pos)` de posições reais.
/// - `heat_ambient_height.jsonl`: geradores funcionando em alturas, biomas e colunas diferentes, lava sempre cheia e energia zerada a cada
///   tick (o gerador produz o tempo todo), a temperatura do ar de cada face, o calor, a produção e a perda para o ambiente.
/// - `heat_ambient_cache.jsonl`: o bioma de baixo do gerador muda no meio do cenário e o tile é recriado do NBT depois: o ar que o
///   tile lê só muda quando o objeto é novo (`CachedAmbientTemperature`).
///
/// Posição de cada gerador: o gerador é posto direto no mundo (`setBlockAndUpdate`), em colunas de 4 por 4 blocos com o bioma trocado
/// por `FillBiomeCommand.fill` (o mesmo código do `/fillbiome`), para que o gerador e as faces dele fiquem no mesmo bioma, menos no
/// gerador de fronteira.
@ForEachTest(groups = "trace")
public class HeatAmbientTraceTest {

    private static final int TIMEOUT = 120 * SharedConstants.TICKS_PER_SECOND;
    private static final long LAVA = 10_000;
    private static final Direction[] SIDES = Direction.values();

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(48, 1, 36).fill(0, 0, 0, 47, 0, 35, Blocks.STONE.defaultBlockState()));
    }

    private static ResourceKey<Biome> biomeKey(String id) {
        return ResourceKey.create(Registries.BIOME, Identifier.parse(id));
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_ambient")
    @TestHolder(description = "Records the ambient temperature of the Heat Generator at several heights, biomes and columns, and the generators running on it.")
    public static void heatAmbientHeight(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("heat_ambient_height");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  scene.begin(helper);
                  int sea = helper.getLevel().getSeaLevel();
                  int floor = helper.absolutePos(new BlockPos(0, 1, 0)).getY();
                  //Acima e abaixo de `seaLevel + 17`, perto do teto da construção, em biomas quentes, frios e no congelado.
                  scene.add("floor_desert", 4, 4, floor, "minecraft:desert", LAVA);
                  scene.add("snow_level_plains", 16, 4, sea + 17, "minecraft:plains", LAVA);
                  scene.add("above_snow_level_plains", 28, 4, sea + 18, "minecraft:plains", LAVA);
                  scene.add("y40_snowy", 40, 4, 40, "minecraft:snowy_plains", LAVA);
                  scene.add("y100_desert", 4, 16, 100, "minecraft:desert", LAVA);
                  scene.add("y100_jungle", 16, 16, 100, "minecraft:jungle", LAVA);
                  scene.add("y200_frozen_ocean", 28, 16, 200, "minecraft:frozen_ocean", LAVA);
                  scene.add("y300_desert", 40, 16, 300, "minecraft:desert", LAVA);
                  scene.add("y300_taiga_idle", 28, 28, 300, "minecraft:taiga", 0);
                  scene.add("y150_badlands_idle", 40, 28, 150, "minecraft:badlands", 0);
                  //Na fronteira entre dois quadrados de 4 por 4, a face leste e o bloco são de biomas diferentes.
                  scene.addBoundary("y120_boundary", 4, 28, 120, "minecraft:desert", "minecraft:snowy_plains", LAVA);
                  scene.addBoundary("y70_boundary", 16, 28, 70, "minecraft:jungle", "minecraft:desert", LAVA);
              })
              .thenExecuteAfter(5, scene::start)
              .thenExecuteAfter(160, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_ambient")
    @TestHolder(description = "Records the ambient temperature staying cached while the biome under the Heat Generator changes, until the tile is recreated from its NBT.")
    public static void heatAmbientCache(final DynamicTest test) {
        template(test);
        Scene scene = new Scene("heat_ambient_cache");
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  scene.begin(helper);
                  scene.add("y200_desert", 4, 4, 200, "minecraft:desert", LAVA);
                  scene.add("y60_snowy", 16, 4, 60, "minecraft:snowy_plains", LAVA);
                  scene.add("y200_jungle_control", 28, 4, 200, "minecraft:jungle", LAVA);
              })
              .thenExecuteAfter(5, scene::start)
              .thenExecuteAfter(60, () -> scene.apply("biome 0 minecraft:snowy_plains, biome 1 minecraft:desert"))
              .thenExecuteAfter(60, () -> scene.apply("reload 0"))
              .thenExecuteAfter(60, () -> scene.apply("reload 1"))
              .thenExecuteAfter(60, () -> scene.apply("biome 2 minecraft:desert, reload 2"))
              .thenExecuteAfter(60, scene::write)
              .thenSucceed());
    }


    /// Os mesmos geradores na mesma coluna, em alturas de baixo a perto do teto, num bioma só (todo chunk carregado em volta da estrutura
    /// leva o bioma, para o zoom do `BiomeManager` não misturar nada): é o cenário que o jogo do port repete nas mesmas coordenadas.
    private static void game(DynamicTest test, String name, String biome) {
        template(test);
        Scene scene = new Scene(name);
        test.eventListeners().forge().addListener((final ServerTickEvent.Post event) -> scene.record());
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> {
                  scene.begin(helper);
                  scene.fillChunks(biome);
                  int sea = helper.getLevel().getSeaLevel();
                  int floor = helper.absolutePos(new BlockPos(0, 1, 0)).getY();
                  //Nenhum par de geradores encostados: dois Heat Generators vizinhos trocam calor pela face (`simulateAdjacent`).
                  for (int y : new int[]{floor, sea + 17, sea + 19, 40, 100, 200, 300}) {
                      scene.add("y" + y, 24, 16, y, biome, LAVA);
                  }
              })
              .thenExecuteAfter(5, scene::start)
              .thenExecuteAfter(200, scene::write)
              .thenSucceed());
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_ambient_desert")
    @TestHolder(description = "Records Heat Generators at seven heights in one column of a desert.")
    public static void heatAmbientGameDesert(final DynamicTest test) {
        game(test, "heat_ambient_game_desert", "minecraft:desert");
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_ambient_frozen")
    @TestHolder(description = "Records Heat Generators at seven heights in one column of a frozen ocean, whose temperature modifier reads noise by position.")
    public static void heatAmbientGameFrozen(final DynamicTest test) {
        game(test, "heat_ambient_game_frozen", "minecraft:frozen_ocean");
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = "trace_ambient")
    @TestHolder(description = "Records Biome.getTemperature and HeatAPI.getAmbientTemp over biomes, columns, heights and sea levels, and the sea level of each dimension.")
    public static void heatAmbientTable(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> helper.startSequence()
              .thenExecute(() -> table(helper))
              .thenSucceed());
    }

    private static final String[] TABLE_BIOMES = {
          "minecraft:plains", "minecraft:desert", "minecraft:snowy_plains", "minecraft:jungle", "minecraft:frozen_ocean",
          "minecraft:deep_frozen_ocean", "minecraft:ice_spikes", "minecraft:nether_wastes", "minecraft:badlands",
          "minecraft:windswept_hills", "minecraft:taiga", "minecraft:meadow"};
    private static final int[][] TABLE_XZ = {{0, 0}, {13, -7}, {-250, 417}, {1234, -5678}, {-90000, 31337}, {2999, 3001}};
    private static final int[] TABLE_SEA_LEVELS = {63, 32, 0, -63};
    /// Alturas em relação ao nível do mar: o nível, o limite (`+17`) dos dois lados, e os acima dele até o teto de 320.
    private static final int[] TABLE_HEIGHTS = {-60, 0, 16, 17, 18, 19, 40, 100, 200, 250};

    @SuppressWarnings("deprecation")
    private static void table(MekGameTestHelper helper) {
        ServerLevel level = helper.getLevel();
        Registry<Biome> registry = level.registryAccess().lookupOrThrow(Registries.BIOME);
        List<String> lines = new ArrayList<>();
        StringBuilder levels = new StringBuilder("{\"levels\":{\"overworld\":").append(level.getSeaLevel());
        for (ResourceKey<Level> dimension : List.of(Level.NETHER, Level.END)) {
            ServerLevel other = level.getServer().getLevel(dimension);
            levels.append(",\"").append(dimension.identifier()).append("\":").append(other == null ? "null" : String.valueOf(other.getSeaLevel()));
        }
        lines.add(levels.append("}}").toString());
        StringBuilder biomes = new StringBuilder("{\"biomes\":{");
        boolean first = true;
        for (String id : TABLE_BIOMES) {
            Biome biome = registry.getOrThrow(biomeKey(id)).value();
            biomes.append(first ? "" : ",").append('"').append(id).append("\":{\"temperature\":").append((double) biome.getBaseTemperature())
                  .append(",\"temperature_bits\":").append(Float.floatToRawIntBits(biome.getBaseTemperature()))
                  .append(",\"modifier\":\"").append(biome.getModifiedClimateSettings().temperatureModifier().getName()).append("\"}");
            first = false;
        }
        lines.add(biomes.append("}}").toString());
        //Uma linha por posição: [bioma, x, y, z, nível do mar, bits da temperatura, temperatura do ar]. O x anda 1 000 000 por nível do
        //mar: o cache do bioma (`Biome.temperatureCache`) é por posição e vale para qualquer nível do mar.
        int group = 0;
        for (int sea : TABLE_SEA_LEVELS) {
            for (String id : TABLE_BIOMES) {
                Biome biome = registry.getOrThrow(biomeKey(id)).value();
                for (int[] xz : TABLE_XZ) {
                    for (int height : TABLE_HEIGHTS) {
                        BlockPos pos = new BlockPos(xz[0] + group * 1_000_000, sea + height, xz[1]);
                        float temperature = biome.getTemperature(pos, sea);
                        double ambient = HeatAPI.getAmbientTemp((double) temperature);
                        lines.add("{\"row\":[\"" + id + "\"," + pos.getX() + "," + pos.getY() + "," + pos.getZ() + "," + sea + ","
                              + Float.floatToRawIntBits(temperature) + "," + ambient + "]}");
                    }
                }
            }
            group++;
        }
        //O modificador `frozen` varre uma grade de posições: o limiar do gelo e o da variação pequena só aparecem quando a soma cai perto
        //deles. Abaixo de `seaLevel + 17`, sem o ajuste de altura.
        for (String id : new String[]{"minecraft:frozen_ocean", "minecraft:deep_frozen_ocean"}) {
            Biome biome = registry.getOrThrow(biomeKey(id)).value();
            for (int ix = 0; ix < 30; ix++) {
                for (int iz = 0; iz < 30; iz++) {
                    BlockPos pos = new BlockPos(5_000_000 + ix * 7 - 140, 62, -200 + iz * 11);
                    float temperature = biome.getTemperature(pos, 63);
                    lines.add("{\"scan\":[\"" + id + "\"," + pos.getX() + "," + pos.getY() + "," + pos.getZ() + ",63,"
                          + Float.floatToRawIntBits(temperature) + "," + HeatAPI.getAmbientTemp((double) temperature) + "]}");
                }
            }
        }
        //O cache do bioma não olha o nível do mar: a mesma posição com outro nível do mar devolve o valor do primeiro.
        Biome plains = registry.getOrThrow(biomeKey("minecraft:plains")).value();
        BlockPos aliased = new BlockPos(7_000_000, 150, 7_000_000);
        for (int sea : new int[]{63, -63, 32, 63}) {
            float temperature = plains.getTemperature(aliased, sea);
            lines.add("{\"aliased\":[\"minecraft:plains\"," + aliased.getX() + "," + aliased.getY() + "," + aliased.getZ() + "," + sea + ","
                  + Float.floatToRawIntBits(temperature) + "," + HeatAPI.getAmbientTemp((double) temperature) + "]}");
        }
        //O caminho inteiro (`getAmbientTemp(level, pos)`) em posições reais do nível do teste, com o bioma que o nível tem ali.
        BlockPos origin = helper.absolutePos(BlockPos.ZERO);
        for (int dy : new int[]{1, 20, 80, 150, 250}) {
            BlockPos pos = new BlockPos(origin.getX() + 3, dy, origin.getZ() + 5);
            Holder<Biome> holder = level.getBiome(pos);
            lines.add("{\"world\":[\"" + holder.unwrapKey().orElseThrow().identifier() + "\"," + pos.getX() + "," + pos.getY() + "," + pos.getZ() + ","
                  + level.getSeaLevel() + "," + Float.floatToRawIntBits(holder.value().getTemperature(pos, level.getSeaLevel())) + ","
                  + HeatAPI.getAmbientTemp(level, pos) + "]}");
        }
        write("heat_ambient_table", lines);
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

    private static final class Gen {
        final String label;
        final BlockPos pos;
        final String biome;
        final String east;

        Gen(String label, BlockPos pos, String biome, String east) {
            this.label = label;
            this.pos = pos;
            this.biome = biome;
            this.east = east;
        }
    }

    private static final class Scene {
        private final String name;
        private final List<Gen> gens = new ArrayList<>();
        private final List<String> lines = new ArrayList<>();
        private final List<String> marks = new ArrayList<>();
        /// Os biomas vistos, na ordem: as linhas guardam o índice nesta lista (`biome_ids` do cabeçalho).
        private final Map<String, String> biomesSeen = new LinkedHashMap<>();
        private final List<String> biomeIds = new ArrayList<>();
        private final double[] heatBefore = new double[32];
        private MekGameTestHelper helper;
        private boolean recording;
        private int tick;

        Scene(String name) {
            this.name = name;
        }

        void begin(MekGameTestHelper helper) {
            this.helper = helper;
        }

        ServerLevel level() {
            return helper.getLevel();
        }

        /// A posição absoluta da coluna: `rx` e `rz` pedem a coluna relativa à estrutura, e a coluna anda até x e z múltiplos de 4. O
        /// `BiomeManager.getBiome` lê o quadrado de 4 por 4 mais próximo de (x - 2, z - 2), com sorteio: o quadrado `k` vale para os
        /// blocos de 4k - 2 a 4k + 2, então o bloco no meio dele e os vizinhos de 1 bloco ficam no mesmo bioma.
        private BlockPos column(int rx, int rz, int y) {
            BlockPos origin = helper.absolutePos(BlockPos.ZERO);
            int x = origin.getX() + rx;
            int z = origin.getZ() + rz;
            x += Math.floorMod(-x, 4);
            z += Math.floorMod(-z, 4);
            return new BlockPos(x, y, z);
        }

        /// O bioma em três quadrados por três por três em volta do quadrado de `at` (o sorteio do zoom pode ler um vizinho).
        private void fillBiome(BlockPos at, String id) {
            fillBox(at.offset(-4, -4, -4), at.offset(4, 4, 4), id);
        }

        private void fillBox(BlockPos from, BlockPos to, String id) {
            Holder<Biome> biome = level().registryAccess().lookupOrThrow(Registries.BIOME).getOrThrow(biomeKey(id));
            FillBiomeCommand.fill(level(), from, to, biome);
        }

        /// Todo chunk carregado num quadrado de 5 por 5 chunks em volta da estrutura passa a ter este bioma (`fillBiomesFromNoise`, o que o
        /// `/fillbiome` faz por chunk).
        void fillChunks(String id) {
            Holder<Biome> biome = level().registryAccess().lookupOrThrow(Registries.BIOME).getOrThrow(biomeKey(id));
            BlockPos origin = helper.absolutePos(BlockPos.ZERO);
            List<ChunkAccess> chunks = new ArrayList<>();
            for (int dz = -2; dz <= 2; dz++) {
                for (int dx = -2; dx <= 2; dx++) {
                    ChunkAccess chunk = level().getChunk((origin.getX() >> 4) + 1 + dx, (origin.getZ() >> 4) + 1 + dz, ChunkStatus.FULL, false);
                    if (chunk != null) {
                        chunk.fillBiomesFromNoise((quartX, quartY, quartZ) -> biome);
                        chunk.markUnsaved();
                        chunks.add(chunk);
                    }
                }
            }
            level().getChunkSource().chunkMap.resendBiomesForChunks(chunks);
        }

        void add(String label, int rx, int rz, int y, String biome, long lava) {
            BlockPos pos = column(rx, rz, y);
            fillBiome(pos, biome);
            place(label, pos, biome, biome, lava);
        }

        /// O gerador entre dois quadrados: x = 4k + 2, com os quadrados k - 1 e k de `biome` e k + 1 e k + 2 de `east`.
        void addBoundary(String label, int rx, int rz, int y, String biome, String east, long lava) {
            BlockPos base = column(rx, rz, y);
            fillBox(base.offset(-4, -4, -4), base.offset(0, 4, 4), biome);
            fillBox(base.offset(4, -4, -4), base.offset(8, 4, 4), east);
            place(label, base.offset(2, 0, 0), biome, east, lava);
        }

        private void place(String label, BlockPos pos, String biome, String east, long lava) {
            if (!level().isInsideBuildHeight(pos)) {
                throw new IllegalStateException("fora da construção: " + pos);
            }
            level().setBlockAndUpdate(pos, GeneratorsBlocks.HEAT_GENERATOR.defaultState());
            gens.add(new Gen(label, pos, biome, east));
            tile(gens.size() - 1).lavaTank.setContents(lava == 0 ? FluidResource.EMPTY : FluidResource.of(Fluids.LAVA), lava, null);
        }

        TileEntityHeatGenerator tile(int index) {
            return (TileEntityHeatGenerator) level().getBlockEntity(gens.get(index).pos);
        }

        /// O capacitor guarda o calor desde o primeiro tick: o rastro parte do que ele tem agora, com o ar de cada face já lido.
        void start() {
            for (int i = 0; i < gens.size(); i++) {
                tile(i).energyContainer().setEnergy(0, null);
                heatBefore[i] = heat(i);
            }
            recording = true;
        }

        private double heat(int index) {
            try {
                java.lang.reflect.Field field = TileEntityHeatGenerator.class.getDeclaredField("heatCapacitor");
                field.setAccessible(true);
                return ((mekanism.common.capabilities.heat.BasicHeatCapacitor) field.get(tile(index))).getHeat();
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        }

        /// Uma marca: o que o cenário fez depois daquele tick. As partes separadas por vírgula acontecem juntas.
        void apply(String mark) {
            marks.add(mark);
            for (String action : mark.split(", ")) {
                String[] words = action.split(" ");
                switch (words[0]) {
                    case "biome" -> fillBiome(gens.get(Integer.parseInt(words[1])).pos, words[2]);
                    case "reload" -> reload(Integer.parseInt(words[1]));
                    default -> throw new IllegalStateException(action);
                }
            }
        }

        /// O que o jogo faz ao descarregar e carregar o chunk: grava o tile e cria outro do NBT (`BlockEntity.loadStatic`).
        private void reload(int index) {
            TileEntityHeatGenerator old = tile(index);
            BlockPos pos = gens.get(index).pos;
            CompoundTag tag = old.saveWithFullMetadata(level().registryAccess());
            BlockEntity loaded = BlockEntity.loadStatic(pos, level().getBlockState(pos), tag, level().registryAccess());
            if (!(loaded instanceof TileEntityHeatGenerator) || loaded == old) {
                throw new IllegalStateException("loadStatic gave " + loaded);
            }
            LevelChunk chunk = level().getChunkAt(pos);
            old.onChunkUnloaded();
            chunk.removeBlockEntity(pos);
            chunk.addAndRegisterBlockEntity(loaded);
        }

        private int biomeAt(BlockPos pos) {
            Holder<Biome> holder = level().getBiome(pos);
            String id = holder.unwrapKey().orElseThrow().identifier().toString();
            if (!biomesSeen.containsKey(id)) {
                Biome biome = holder.value();
                biomesSeen.put(id, "{\"temperature\":" + (double) biome.getBaseTemperature() + ",\"temperature_bits\":"
                      + Float.floatToRawIntBits(biome.getBaseTemperature()) + ",\"modifier\":\""
                      + biome.getModifiedClimateSettings().temperatureModifier().getName() + "\"}");
                biomeIds.add(id);
            }
            return biomeIds.indexOf(id);
        }

        void record() {
            if (!recording) {
                return;
            }
            StringBuilder line = new StringBuilder("{\"t\":").append(tick).append(",\"marks\":[");
            for (int i = 0; i < marks.size(); i++) {
                line.append(i == 0 ? "" : ",").append('"').append(marks.get(i)).append('"');
            }
            marks.clear();
            line.append("],\"gens\":[");
            for (int i = 0; i < gens.size(); i++) {
                TileEntityHeatGenerator tile = tile(i);
                BlockPos pos = gens.get(i).pos;
                line.append(i == 0 ? "" : ",").append("{\"heat_before\":").append(tick == 0 ? heatBefore[i] : 0)
                      .append(",\"heat\":").append(heat(i))
                      .append(",\"temperature\":").append(tile.getTemperature())
                      //O ar que o tile lê (o cache do objeto) e o que o mundo daria agora.
                      .append(",\"ambient\":").append(tile.getAmbientTemperature(null))
                      .append(",\"ambient_direct\":").append(HeatAPI.getAmbientTemp(level(), pos))
                      .append(",\"ambient_faces\":[");
                for (Direction side : SIDES) {
                    line.append(side.ordinal() == 0 ? "" : ",").append(tile.getAmbientTemperature(side));
                }
                line.append("],\"biomes\":[").append(biomeAt(pos));
                for (Direction side : SIDES) {
                    line.append(',').append(biomeAt(pos.relative(side)));
                }
                line.append("],\"energy\":").append(tile.energyContainer().getAmountAsLong())
                                            .append(",\"lava\":").append(tile.lavaTank.amountAsLong())
                      .append(",\"produced\":").append(tile.getProductionRate())
                      .append(",\"transfer_loss\":").append(tile.getLastTransferLoss())
                      .append(",\"environment_loss\":").append(tile.getLastEnvironmentLoss())
                      .append(",\"active\":").append(tile.getActive())
                      .append('}');
                //O buffer vazio: o gerador produz no tick seguinte.
                tile.energyContainer().setEnergy(0, null);
            }
            lines.add(line.append("]}").toString());
            tick++;
        }

        void write() {
            recording = false;
            StringBuilder header = new StringBuilder("{\"header\":true,\"sea_level\":").append(level().getSeaLevel())
                  .append(",\"min_y\":").append(level().getMinY()).append(",\"max_y\":").append(level().getMaxY())
                  .append(",\"epsilon\":").append(HeatAPI.EPSILON).append(",\"gens\":[");
            for (int i = 0; i < gens.size(); i++) {
                BlockPos pos = gens.get(i).pos;
                header.append(i == 0 ? "" : ",").append("{\"label\":\"").append(gens.get(i).label).append("\",\"biome\":\"").append(gens.get(i).biome)
                      .append("\",\"east\":\"").append(gens.get(i).east).append("\",\"pos\":[")
                      .append(pos.getX()).append(',').append(pos.getY()).append(',').append(pos.getZ()).append("]}");
            }
            header.append("],\"biome_ids\":[");
            for (int i = 0; i < biomeIds.size(); i++) {
                header.append(i == 0 ? "" : ",").append('"').append(biomeIds.get(i)).append('"');
            }
            header.append("],\"biomes\":{");
            boolean first = true;
            for (Map.Entry<String, String> entry : biomesSeen.entrySet()) {
                header.append(first ? "" : ",").append('"').append(entry.getKey()).append("\":").append(entry.getValue());
                first = false;
            }
            lines.add(0, header.append("}}").toString());
            HeatAmbientTraceTest.write(name, lines);
        }
    }
}
