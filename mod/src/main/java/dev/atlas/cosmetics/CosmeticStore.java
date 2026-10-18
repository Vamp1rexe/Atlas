package dev.atlas.cosmetics;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.Reader;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Set;
import java.util.stream.Collectors;
import org.slf4j.Logger;
import org.slf4j.LoggerFactory;

/** Reads what the launcher hands over through -Datlas.* system properties. */
final class CosmeticStore {
	private static final Logger LOGGER = LoggerFactory.getLogger("atlas-cosmetics");
	private static volatile List<CosmeticModel> equipped = List.of();

	private CosmeticStore() { }

	static List<CosmeticModel> equipped() {
		return equipped;
	}

	static void load() {
		String dir = System.getProperty("atlas.cosmetics.dir");
		String catalog = System.getProperty("atlas.cosmetics.catalog");
		String optionsFile = System.getProperty("atlas.options");
		String ids = System.getProperty("atlas.equipped", "");
		if (dir == null || catalog == null || ids.isBlank()) {
			LOGGER.info("No cosmetics equipped (not started from Atlas, or nothing equipped).");
			return;
		}

		JsonObject options = new JsonObject();
		if (optionsFile != null) {
			try (Reader r = Files.newBufferedReader(Path.of(optionsFile))) {
				options = JsonParser.parseReader(r).getAsJsonObject();
			} catch (Exception e) {
				LOGGER.warn("Could not read cosmetic options: {}", e.toString());
			}
		}

		Set<String> wanted = Arrays.stream(ids.split(",")).map(String::trim).filter(s -> !s.isEmpty()).collect(Collectors.toSet());
		Path base = Path.of(dir);
		List<CosmeticModel> loaded = new ArrayList<>();
		try (Reader r = Files.newBufferedReader(Path.of(catalog))) {
			for (JsonElement el : JsonParser.parseReader(r).getAsJsonObject().getAsJsonArray("cosmetics")) {
				JsonObject c = el.getAsJsonObject();
				String id = c.get("id").getAsString();
				if (!wanted.contains(id)) continue;

				float scale = 1f;
				String color = c.has("default_color") ? c.get("default_color").getAsString() : "";
				if (options.has(id) && options.get(id).isJsonObject()) {
					JsonObject o = options.getAsJsonObject(id);
					if (o.has("scale")) scale = Math.max(0.2f, Math.min(4f, o.get("scale").getAsFloat()));
					if (o.has("color") && !o.get("color").getAsString().isBlank()) color = o.get("color").getAsString();
				}
				try {
					loaded.add(CosmeticModel.load(id, base.resolve(c.get("model").getAsString()),
							base.resolve(c.get("texture").getAsString()), scale, parseColor(color)));
				} catch (Exception e) {
					LOGGER.warn("Could not load cosmetic {}: {}", id, e.toString());
				}
			}
		} catch (Exception e) {
			LOGGER.warn("Could not read cosmetics catalog: {}", e.toString());
		}
		equipped = List.copyOf(loaded);
		LOGGER.info("Loaded {} cosmetic(s): {}", loaded.size(), loaded.stream().map(CosmeticModel::id).toList());
	}

	/** "#rrggbb" to 0xRRGGBB; anything unreadable means no tint (white). */
	static int parseColor(String hex) {
		if (hex == null) return 0xFFFFFF;
		String s = hex.trim();
		if (s.startsWith("#")) s = s.substring(1);
		if (s.length() != 6) return 0xFFFFFF;
		try {
			return Integer.parseInt(s, 16);
		} catch (NumberFormatException e) {
			return 0xFFFFFF;
		}
	}
}
