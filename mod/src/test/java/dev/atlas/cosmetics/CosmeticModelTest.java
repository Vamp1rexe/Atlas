package dev.atlas.cosmetics;

import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.Reader;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import org.junit.jupiter.api.Test;

class CosmeticModelTest {
	private static final Path COSMETICS = Path.of("..", "cosmetics");

	private static int quads(List<CosmeticModel.Bone> bones) {
		int n = 0;
		for (CosmeticModel.Bone b : bones) n += b.quads.size() + quads(b.children);
		return n;
	}

	private static int bones(List<CosmeticModel.Bone> bones) {
		int n = 0;
		for (CosmeticModel.Bone b : bones) n += 1 + bones(b.children);
		return n;
	}

	@Test
	void everyBuiltInCosmeticLoadsAndStaysWithinBudget() throws Exception {
		JsonObject catalog;
		try (Reader r = Files.newBufferedReader(COSMETICS.resolve("catalog.json"))) {
			catalog = JsonParser.parseReader(r).getAsJsonObject();
		}
		int checked = 0;
		for (JsonElement el : catalog.getAsJsonArray("cosmetics")) {
			JsonObject c = el.getAsJsonObject();
			String id = c.get("id").getAsString();
			CosmeticModel m = CosmeticModel.load(id, COSMETICS.resolve(c.get("model").getAsString()),
					COSMETICS.resolve(c.get("texture").getAsString()), 1f, 0xFFFFFF);
			int q = quads(m.roots());
			System.out.println("cosmetic " + id + ": " + bones(m.roots()) + " bones, " + q + " quads");
			assertTrue(q > 0, id + " has no geometry");
			assertTrue(q < 4000, id + " is too heavy: " + q + " quads");
			checked++;
		}
		assertTrue(checked >= 5, "catalog is missing entries");
	}
}
