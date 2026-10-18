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
		String ids = System.getProperty("atlas.equipped", "");
		if (dir == null || catalog == null || ids.isBlank()) {
			LOGGER.info("No cosmetics equipped (not started from Atlas, or nothing equipped).");
			return;
		}

		Set<String> wanted = Arrays.stream(ids.split(",")).map(String::trim).filter(s -> !s.isEmpty()).collect(Collectors.toSet());
		Path base = Path.of(dir);
		List<CosmeticModel> loaded = new ArrayList<>();
		try (Reader r = Files.newBufferedReader(Path.of(catalog))) {
			for (JsonElement el : JsonParser.parseReader(r).getAsJsonObject().getAsJsonArray("cosmetics")) {
				JsonObject c = el.getAsJsonObject();
				String id = c.get("id").getAsString();
				if (!wanted.contains(id)) continue;
				try {
					loaded.add(CosmeticModel.load(id, base.resolve(c.get("model").getAsString()), base.resolve(c.get("texture").getAsString())));
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
}
