package mekanism.common.tests.network;

import com.google.gson.JsonObject;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import mekanism.api.RelativeSide;
import mekanism.api.text.EnumColor;
import mekanism.common.content.network.transmitter.LogisticalTransporter;
import mekanism.common.lib.transmitter.TransmissionType;
import mekanism.common.registries.MekanismBlocks;
import mekanism.common.tests.helpers.MekGameTestHelper;
import mekanism.common.tile.base.TileEntityMekanism;
import mekanism.common.tile.component.config.ConfigInfo;
import mekanism.common.tile.component.config.DataType;
import mekanism.common.tile.interfaces.ISideConfiguration;
import mekanism.common.tile.transmitter.TileEntityLogisticalTransporter;
import mekanism.common.util.WorldUtils;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.ChestBlockEntity;
import net.minecraft.world.level.block.state.properties.BlockStateProperties;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.testframework.DynamicTest;
import net.neoforged.testframework.annotation.ForEachTest;
import net.neoforged.testframework.annotation.TestHolder;
import net.neoforged.testframework.gametest.GameTest;
import net.neoforged.testframework.gametest.StructureTemplateBuilder;

/// Frente `fase3-faltas`: grava para onde vai uma pilha que o ejetor de uma máquina manda a um transportador logístico, conforme a cor de saída do ejetor, as cores dos transportadores e a entrada
/// estrita e a cor de entrada da máquina de destino. Cada cena: um Crusher de origem (a frente em `OUTPUT`, ejetando) com cobblestone no slot de saída, dois transportadores sem cor em
/// linha, e a partir do segundo dois ramos (norte e sul) que levam a um baú ou a um Crusher de destino. Um `runGameTestServer`, o arquivo `ejector_color.jsonl`: uma linha por cena com o que
/// cada ponta tem depois de 400 ticks.
@ForEachTest(groups = "trace")
public class EjectorColorTraceTest {

    private static final int TIMEOUT = 900 * SharedConstants.TICKS_PER_SECOND;
    private static final String BATCH = "trace_ejector_color";
    private static final String COMMIT = "b2d04e166";
    private static final int ITEMS = 20;
    private static final int RUN = 400;
    private static final BlockPos SOURCE = new BlockPos(1, 1, 3);
    private static final BlockPos T0 = new BlockPos(2, 1, 3);
    private static final BlockPos T1 = new BlockPos(3, 1, 3);
    private static final BlockPos NORTH_T = new BlockPos(3, 1, 2);
    private static final BlockPos SOUTH_T = new BlockPos(3, 1, 4);
    private static final BlockPos NORTH_END = new BlockPos(3, 1, 1);
    private static final BlockPos SOUTH_END = new BlockPos(3, 1, 5);

    private record Scene(String name, EnumColor outputColor, EnumColor northColor, EnumColor southColor, boolean machineNorth, boolean strict, EnumColor inputColor, boolean southBranch) {
    }

    private static final List<Scene> SCENES = List.of(
          new Scene("color_blue", EnumColor.DARK_BLUE, EnumColor.BLACK, EnumColor.DARK_BLUE, false, false, null, true),
          new Scene("color_black", EnumColor.BLACK, EnumColor.BLACK, EnumColor.DARK_BLUE, false, false, null, true),
          new Scene("color_none", null, EnumColor.BLACK, EnumColor.DARK_BLUE, false, false, null, true),
          new Scene("color_other", EnumColor.RED, EnumColor.BLACK, EnumColor.DARK_BLUE, false, false, null, true),
          new Scene("strict_match", EnumColor.DARK_BLUE, null, null, true, true, EnumColor.DARK_BLUE, false),
          new Scene("strict_none", null, null, null, true, true, EnumColor.DARK_BLUE, false),
          new Scene("strict_other", EnumColor.BLACK, null, null, true, true, EnumColor.DARK_BLUE, false),
          new Scene("loose_input_color", null, null, null, true, false, EnumColor.DARK_BLUE, false),
          new Scene("strict_side_without_color", EnumColor.BLACK, null, null, true, true, null, false),
          new Scene("strict_with_chest_beside", null, null, null, true, true, EnumColor.DARK_BLUE, true));

    private static void template(DynamicTest test) {
        test.registerGameTestTemplate(() -> StructureTemplateBuilder.withSize(8, 4, 8).fill(0, 0, 0, 7, 0, 7, Blocks.STONE.defaultBlockState()));
    }

    private static TileEntityMekanism crusher(MekGameTestHelper helper, BlockPos rel, Direction facing) {
        helper.setBlock(rel, MekanismBlocks.CRUSHER.defaultState().setValue(BlockStateProperties.HORIZONTAL_FACING, facing));
        TileEntityMekanism tile = WorldUtils.getTileEntity(TileEntityMekanism.class, helper.getLevel(), helper.absolutePos(rel));
        tile.applyComponentsFromItemStack(new ItemStack(MekanismBlocks.CRUSHER.asItem()));
        ConfigInfo info = ((ISideConfiguration) tile).getConfig().getConfig(TransmissionType.ITEM);
        for (RelativeSide relative : RelativeSide.VALUES) {
            info.setDataType(DataType.NONE, relative);
        }
        return tile;
    }

    private static void transporter(MekGameTestHelper helper, BlockPos rel, EnumColor color) {
        helper.setBlock(rel, MekanismBlocks.BASIC_LOGISTICAL_TRANSPORTER.defaultState());
        if (color != null) {
            TileEntityLogisticalTransporter tile = WorldUtils.getTileEntity(TileEntityLogisticalTransporter.class, helper.getLevel(), helper.absolutePos(rel));
            LogisticalTransporter transmitter = tile.getTransmitter();
            transmitter.setColor(color);
        }
    }

    private static void clear(MekGameTestHelper helper) {
        for (int x = 0; x < 8; x++) {
            for (int z = 0; z < 8; z++) {
                for (int y = 1; y <= 3; y++) {
                    helper.setBlock(new BlockPos(x, y, z), Blocks.AIR.defaultBlockState());
                }
            }
        }
    }

    private static void build(MekGameTestHelper helper, Scene scene) {
        clear(helper);
        TileEntityMekanism source = crusher(helper, SOURCE, Direction.EAST);
        ConfigInfo info = ((ISideConfiguration) source).getConfig().getConfig(TransmissionType.ITEM);
        info.setDataType(DataType.OUTPUT, RelativeSide.FRONT);
        info.setEjecting(true);
        ((ISideConfiguration) source).getEjector().setOutputColor(scene.outputColor);
        source.getInventorySlots().get(1).setContents(ItemResource.of(Items.COBBLESTONE), ITEMS, null);
        transporter(helper, T0, null);
        transporter(helper, T1, null);
        transporter(helper, NORTH_T, scene.northColor);
        if (scene.southBranch) {
            transporter(helper, SOUTH_T, scene.southColor);
            helper.setBlock(SOUTH_END, Blocks.CHEST.defaultBlockState());
        }
        if (scene.machineNorth) {
            TileEntityMekanism destination = crusher(helper, NORTH_END, Direction.EAST);
            RelativeSide side = RelativeSide.fromDirections(destination.getDirection(), Direction.SOUTH);
            ((ISideConfiguration) destination).getConfig().getConfig(TransmissionType.ITEM).setDataType(DataType.INPUT, side);
            ((ISideConfiguration) destination).getEjector().setStrictInput(scene.strict);
            ((ISideConfiguration) destination).getEjector().setInputColor(side, scene.inputColor);
        } else {
            helper.setBlock(NORTH_END, Blocks.CHEST.defaultBlockState());
        }
    }

    private static int count(MekGameTestHelper helper, BlockPos rel, int slot) {
        if (helper.getLevel().getBlockEntity(helper.absolutePos(rel)) instanceof ChestBlockEntity chest) {
            int total = 0;
            for (int i = 0; i < chest.getContainerSize(); i++) {
                total += chest.getItem(i).getCount();
            }
            return total;
        }
        TileEntityMekanism tile = WorldUtils.getTileEntity(TileEntityMekanism.class, helper.getLevel(), helper.absolutePos(rel));
        return tile == null ? -1 : tile.getInventorySlots().get(slot).asItemStack().getCount();
    }

    @GameTest(setupTicks = SharedConstants.TICKS_PER_SECOND, timeoutTicks = TIMEOUT, batch = BATCH)
    @TestHolder(description = "Records where the items go when an ejector with an output color feeds a transporter network with colored branches and a machine with strict input.")
    public static void ejectorColor(final DynamicTest test) {
        template(test);
        test.onGameTest(MekGameTestHelper.class, helper -> {
            List<String> lines = new ArrayList<>();
            lines.add("{\"meta\":\"commit " + COMMIT + ". A Crusher facing east with " + ITEMS + " cobblestone in its output slot, ejecting items through its front, two basic transporters east of it and, from the second, a north "
                  + "and a south branch. Per scene: the output color of the source ejector (output), the colors of the branches (north, south), whether the north end is a Crusher (machine) with strict input and "
                  + "an input color on the side that faces the branch, or a chest. After " + RUN + " ticks: source (slot 1 of the source), north (the chest items or slot 0 of the destination Crusher), south (the chest items).\"}");
            var sequence = helper.startSequence();
            for (Scene scene : SCENES) {
                sequence.thenExecuteAfter(1, () -> build(helper, scene));
                sequence.thenExecuteAfter(RUN, () -> {
                    JsonObject line = new JsonObject();
                    line.addProperty("case", scene.name);
                    line.addProperty("output", scene.outputColor == null ? null : scene.outputColor.name());
                    line.addProperty("north_color", scene.northColor == null ? null : scene.northColor.name());
                    line.addProperty("south_color", scene.southBranch && scene.southColor != null ? scene.southColor.name() : null);
                    line.addProperty("machine", scene.machineNorth);
                    line.addProperty("strict", scene.strict);
                    line.addProperty("input_color", scene.inputColor == null ? null : scene.inputColor.name());
                    line.addProperty("source", count(helper, SOURCE, 1));
                    line.addProperty("north", count(helper, NORTH_END, 0));
                    line.addProperty("south", scene.southBranch ? count(helper, SOUTH_END, 0) : -1);
                    lines.add(line.toString());
                });
            }
            sequence.thenExecuteAfter(1, () -> {
                try {
                    Path file = Path.of("trace", "ejector_color.jsonl");
                    Files.createDirectories(file.getParent());
                    Files.write(file, lines);
                } catch (IOException e) {
                    throw new IllegalStateException(e);
                }
            });
            sequence.thenSucceed();
        });
    }
}
