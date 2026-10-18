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
	private static final float DEG = (float) (Math.PI / 180.0);
	private final Quaternionf rot = new Quaternionf();

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
		float seconds = (float) ((System.nanoTime() / 1_000_000L) % 3_600_000L) / 1000f;
		float move = Math.max(0f, Math.min(1f, limbDistance));

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
				case "spin" -> poseStack.mulPose(rot.rotationY(seconds * c.speed()));
				default -> { }
			}

			for (CosmeticModel.Bone bone : c.roots()) {
				renderBone(poseStack, nodes, bone, seconds, move, c.scale(), true);
			}

			poseStack.popPose();
		}
	}

	private void renderBone(PoseStack ps, SubmitNodeCollector nodes, CosmeticModel.Bone b, float t, float move, float scale, boolean root) {
		ps.pushPose();
		ps.translate(b.dx, b.dy, b.dz);
		// Roots scale around their own pivot, so wings grow outward but stay on the shoulders.
		if (root && scale != 1f) ps.scale(scale, scale, scale);

		float wave = (float) Math.sin(t * b.speed + b.phase);
		float rx = (b.base[0] + b.amp[0] * wave + b.move[0] * move) * DEG;
		float ry = (b.base[1] + b.amp[1] * wave + b.move[1] * move) * DEG;
		float rz = (b.base[2] + b.amp[2] * wave + b.move[2] * move) * DEG;
		if (rx != 0f || ry != 0f || rz != 0f) ps.mulPose(rot.rotationXYZ(rx, ry, rz));

		if (!b.quads.isEmpty()) {
			nodes.submitCustomGeometry(ps, RenderTypes.debugFilledBox(), (pose, buffer) -> {
				for (CosmeticModel.Quad q : b.quads) {
					float[] p = q.points();
					int color = q.color();
					// Both windings, so the quad shows regardless of back-face culling.
					for (int i = 0; i < 4; i++) {
						buffer.addVertex(pose, p[i * 3], p[i * 3 + 1], p[i * 3 + 2]).setColor(color);
					}
					for (int i = 3; i >= 0; i--) {
						buffer.addVertex(pose, p[i * 3], p[i * 3 + 1], p[i * 3 + 2]).setColor(color);
					}
				}
			});
		}

		for (CosmeticModel.Bone child : b.children) {
			renderBone(ps, nodes, child, t, move, scale, false);
		}
		ps.popPose();
	}
}
