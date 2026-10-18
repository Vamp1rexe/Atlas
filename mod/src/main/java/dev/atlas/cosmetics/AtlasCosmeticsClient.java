package dev.atlas.cosmetics;

import net.fabricmc.api.ClientModInitializer;
import net.fabricmc.fabric.api.client.rendering.v1.LivingEntityRenderLayerRegistrationCallback;
import net.minecraft.client.renderer.entity.player.AvatarRenderer;

public final class AtlasCosmeticsClient implements ClientModInitializer {
	@Override
	@SuppressWarnings({"rawtypes", "unchecked"})
	public void onInitializeClient() {
		CosmeticStore.load();
		LivingEntityRenderLayerRegistrationCallback.EVENT.register((entityType, entityRenderer, helper, context) -> {
			if (entityRenderer instanceof AvatarRenderer) {
				helper.register(new CosmeticLayer((AvatarRenderer) entityRenderer));
			}
		});
	}
}
