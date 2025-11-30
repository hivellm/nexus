"""Tests for multi-database support."""

import pytest
from nexus_sdk import NexusClient


@pytest.mark.asyncio
async def test_list_databases():
    """Test listing databases."""
    async with NexusClient("http://localhost:15474") as client:
        databases = await client.list_databases()
        assert databases.databases is not None
        assert len(databases.databases) > 0
        assert databases.default_database is not None
        # Default database should be in the list
        assert databases.default_database in databases.databases


@pytest.mark.asyncio
async def test_create_and_drop_database():
    """Test creating and dropping a database."""
    async with NexusClient("http://localhost:15474") as client:
        # Create database
        db_name = "test_temp_db"
        create_result = await client.create_database(db_name)
        assert create_result.success is True
        assert create_result.name == db_name

        # Verify it exists
        databases = await client.list_databases()
        assert db_name in databases.databases

        # Drop database
        drop_result = await client.drop_database(db_name)
        assert drop_result.success is True

        # Verify it's gone
        databases = await client.list_databases()
        assert db_name not in databases.databases


@pytest.mark.asyncio
async def test_switch_database():
    """Test switching between databases."""
    async with NexusClient("http://localhost:15474") as client:
        # Create a test database
        db_name = "test_switch_db"
        await client.create_database(db_name)

        try:
            # Get initial database
            initial_db = await client.get_current_database()

            # Switch to test database
            switch_result = await client.switch_database(db_name)
            assert switch_result.success is True

            # Verify we're in the new database
            current_db = await client.get_current_database()
            assert current_db == db_name

            # Switch back
            switch_result = await client.switch_database(initial_db)
            assert switch_result.success is True

            # Verify we're back
            current_db = await client.get_current_database()
            assert current_db == initial_db
        finally:
            # Clean up
            await client.drop_database(db_name)


@pytest.mark.asyncio
async def test_get_database_info():
    """Test getting database information."""
    async with NexusClient("http://localhost:15474") as client:
        # Create a test database
        db_name = "test_info_db"
        await client.create_database(db_name)

        try:
            # Get database info
            db_info = await client.get_database(db_name)
            assert db_info.name == db_name
            assert db_info.path is not None
            assert db_info.node_count >= 0
            assert db_info.relationship_count >= 0
            assert db_info.storage_size >= 0
        finally:
            # Clean up
            await client.drop_database(db_name)


@pytest.mark.asyncio
async def test_data_isolation():
    """Test that data is isolated between databases."""
    async with NexusClient("http://localhost:15474") as client:
        # Create two test databases
        db1_name = "test_isolation_db1"
        db2_name = "test_isolation_db2"
        await client.create_database(db1_name)
        await client.create_database(db2_name)

        try:
            # Switch to db1 and create a node
            await client.switch_database(db1_name)
            result = await client.execute_cypher(
                "CREATE (n:TestNode {name: 'DB1 Node'}) RETURN n",
                None
            )
            assert len(result.rows) == 1

            # Verify node exists in db1
            result = await client.execute_cypher(
                "MATCH (n:TestNode) RETURN count(n) AS count",
                None
            )
            assert result.rows[0]['count'] == 1

            # Switch to db2
            await client.switch_database(db2_name)

            # Verify node does NOT exist in db2 (isolation)
            result = await client.execute_cypher(
                "MATCH (n:TestNode) RETURN count(n) AS count",
                None
            )
            assert result.rows[0]['count'] == 0

            # Create a different node in db2
            result = await client.execute_cypher(
                "CREATE (n:TestNode {name: 'DB2 Node'}) RETURN n",
                None
            )
            assert len(result.rows) == 1

            # Verify only one node in db2
            result = await client.execute_cypher(
                "MATCH (n:TestNode) RETURN count(n) AS count",
                None
            )
            assert result.rows[0]['count'] == 1

            # Switch back to db1
            await client.switch_database(db1_name)

            # Verify still only one node in db1
            result = await client.execute_cypher(
                "MATCH (n:TestNode) RETURN count(n) AS count",
                None
            )
            assert result.rows[0]['count'] == 1
        finally:
            # Clean up
            await client.drop_database(db1_name)
            await client.drop_database(db2_name)


@pytest.mark.asyncio
async def test_client_with_database_parameter():
    """Test creating a client with a specific database."""
    # Create a test database first
    async with NexusClient("http://localhost:15474") as setup_client:
        db_name = "test_param_db"
        await setup_client.create_database(db_name)

        try:
            # Create a client connected to the specific database
            async with NexusClient("http://localhost:15474", database=db_name) as client:
                # Verify we're connected to the right database
                current_db = await client.get_current_database()
                assert current_db == db_name
        finally:
            # Clean up
            await setup_client.drop_database(db_name)


@pytest.mark.asyncio
async def test_cannot_drop_current_database():
    """Test that we cannot drop the currently active database."""
    async with NexusClient("http://localhost:15474") as client:
        # Create a test database
        db_name = "test_no_drop_db"
        await client.create_database(db_name)

        try:
            # Switch to the database
            await client.switch_database(db_name)

            # Try to drop it while it's active - should fail
            with pytest.raises(Exception):
                await client.drop_database(db_name)

            # Switch to a different database
            databases = await client.list_databases()
            default_db = databases.default_database
            await client.switch_database(default_db)

            # Now we should be able to drop it
            drop_result = await client.drop_database(db_name)
            assert drop_result.success is True
        except Exception:
            # Clean up even if test fails
            databases = await client.list_databases()
            if db_name in databases.databases:
                default_db = databases.default_database
                await client.switch_database(default_db)
                await client.drop_database(db_name)
            raise


@pytest.mark.asyncio
async def test_cannot_drop_default_database():
    """Test that we cannot drop the default database."""
    async with NexusClient("http://localhost:15474") as client:
        # Get default database
        databases = await client.list_databases()
        default_db = databases.default_database

        # Try to drop it - should fail
        with pytest.raises(Exception):
            await client.drop_database(default_db)
