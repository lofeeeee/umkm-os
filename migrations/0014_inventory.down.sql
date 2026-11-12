DROP FUNCTION IF EXISTS app.expire_reservations(integer);
DROP TABLE IF EXISTS inventory_reservations;
DROP TABLE IF EXISTS inventory_movements;
DROP TRIGGER IF EXISTS trg_product_variants_level ON product_variants;
DROP FUNCTION IF EXISTS app.create_inventory_level();
DROP TABLE IF EXISTS inventory_levels;
