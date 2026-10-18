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
	/** Skews the sine so the wing snaps through its centre and lingers at the turning points. */
	static final float SKEW = 0.42f;
	/** Time constant (seconds) of the state smoothing. */
	private static final float EASE = 0.18f;

	private final Quaternionf rot = new Quaternionf();
	private long lastNanos;
	private float animTime;
	private float moveSmooth;

	/** Flap shape shared with the generator's preview: sin(x + SKEW*sin x). */
	static float shape(float x) {
		return (float) Math.sin(x + SKEW * Math.sin(x));
	}

	/** Advances the animation clock and the smoothed walking/airborne factor once per frame. */
	private void tick(float targetMove) {
		long now = System.nanoTime();
		float dt = lastNanos == 0 ? 0f : (now - lastNanos) / 1_000_000_000f;
		if (dt < 0.001f) return; // several passes in one frame: advance only once
		lastNanos = now;
		dt = Math.min(dt, 0.1f);
		float k = 1f - (float) Math.exp(-dt / EASE);
		moveSmooth += (targetMove - moveSmooth) * k;
		// Flaps faster while moving; the clock is accumulated so speed changes never jump.
		animTime += dt * (1f + 0.45f * moveSmooth);
	}

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
		float target = Math.max(0f, Math.min(1f, limbDistance));
		if (!mc.player.onGround() && !mc.player.isInWater()) target = 1f; // airborne: wings open up
		tick(target);
		float seconds = animTime;
		float move = moveSmooth;
		// Standing still the wings only breathe; they flap fully once you move.
		float amp = 0.55f + 0.45f * move;

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
				renderBone(poseStack, nodes, bone, seconds, move, amp, c.scale(), true);
			}

			poseStack.popPose();
		}
	}

	private void renderBone(PoseStack ps, SubmitNodeCollector nodes, CosmeticModel.Bone b, float t, float move, float amp, float scale, boolean root) {
		ps.pushPose();
		ps.translate(b.dx, b.dy, b.dz);
		// Roots scale around their own pivot, so wings grow outward but stay on the shoulders.
		if (root && scale != 1f) ps.scale(scale, scale, scale);

		float wave = shape(t * b.speed + b.phase) * amp;
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
			renderBone(ps, nodes, child, t, move, amp, scale, false);
		}
		ps.popPose();
	}
}
