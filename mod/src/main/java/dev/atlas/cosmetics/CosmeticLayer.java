package dev.atlas.cosmetics;

import com.mojang.blaze3d.vertex.PoseStack;
import net.minecraft.client.Minecraft;
import net.minecraft.client.model.player.PlayerModel;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.entity.RenderLayerParent;
import net.minecraft.client.renderer.entity.layers.RenderLayer;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import net.minecraft.client.renderer.rendertype.RenderTypes;
import org.joml.Quaternionf;

/** Draws the cosmetics equipped in Atlas on the local player. */
final class CosmeticLayer extends RenderLayer<AvatarRenderState, PlayerModel> {
	CosmeticLayer(RenderLayerParent<AvatarRenderState, PlayerModel> parent) {
		super(parent);
	}

	@Override
	public void submit(PoseStack poseStack, SubmitNodeCollector nodes, int light, AvatarRenderState state, float limbAngle, float limbDistance) {
		var equipped = CosmeticStore.equipped();
		if (equipped.isEmpty()) return;

		Minecraft mc = Minecraft.getInstance();
		if (mc.player == null || state.id != mc.player.getId()) return;

		PlayerModel model = getParentModel();
		float seconds = (System.currentTimeMillis() % 3_600_000L) / 1000f;

		for (CosmeticModel c : equipped) {
			poseStack.pushPose();

			if (c.attach().equals("body")) {
				model.body.translateAndRotate(poseStack);
			} else {
				model.head.translateAndRotate(poseStack);
			}

			// Model coordinates are pixels (1/16 block) in the attach part's own space.
			poseStack.scale(1f / 16f, 1f / 16f, 1f / 16f);
			poseStack.translate(c.ox(), c.oy(), c.oz());

			switch (c.animation()) {
				case "bob" -> poseStack.translate(0f, (float) Math.sin(seconds * 2f * c.speed()) * c.amount(), 0f);
				case "spin" -> poseStack.mulPose(new Quaternionf().rotationY(seconds * c.speed()));
				default -> { }
			}
			if (c.scale() != 1f) poseStack.scale(c.scale(), c.scale(), c.scale());

			nodes.submitCustomGeometry(poseStack, RenderTypes.debugFilledBox(), (pose, buffer) -> {
				for (CosmeticModel.Quad q : c.quads()) {
					float[] p = q.points();
					// Both windings, so the quad shows regardless of back-face culling.
					for (int i = 0; i < 4; i++) {
						buffer.addVertex(pose, p[i * 3], p[i * 3 + 1], p[i * 3 + 2]).setColor(q.color());
					}
					for (int i = 3; i >= 0; i--) {
						buffer.addVertex(pose, p[i * 3], p[i * 3 + 1], p[i * 3 + 2]).setColor(q.color());
					}
				}
			});

			poseStack.popPose();
		}
	}
}
