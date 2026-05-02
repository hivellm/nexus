using System.Text.Json;
using Xunit;

namespace Nexus.SDK.Tests;

/// <summary>
/// Unit tests for external-id related DTOs and URL construction
/// (phase9_external-node-ids).  These tests run without a live server.
/// </summary>
public class CreateNodeRequestSerializationTests
{
    [Fact]
    public void OmitsExternalIdWhenNull()
    {
        var req = new CreateNodeRequest
        {
            Labels = new List<string> { "Person" },
            Properties = new Dictionary<string, object?> { ["name"] = "Alice" },
            ExternalId = null,
            ConflictPolicy = null,
        };

        var json = JsonSerializer.Serialize(req);
        Assert.DoesNotContain("external_id", json);
        Assert.DoesNotContain("conflict_policy", json);
    }

    [Fact]
    public void IncludesExternalIdWhenSet()
    {
        var req = new CreateNodeRequest
        {
            Labels = new List<string> { "Person" },
            Properties = new Dictionary<string, object?> { ["name"] = "Alice" },
            ExternalId = "str:alice-key",
            ConflictPolicy = "match",
        };

        var json = JsonSerializer.Serialize(req);
        Assert.Contains("\"external_id\":\"str:alice-key\"", json);
        Assert.Contains("\"conflict_policy\":\"match\"", json);
    }

    [Fact]
    public void OmitsConflictPolicyWhenNull()
    {
        var req = new CreateNodeRequest
        {
            Labels = new List<string> { "Person" },
            Properties = new Dictionary<string, object?>(),
            ExternalId = "sha256:abcdef",
            ConflictPolicy = null,
        };

        var json = JsonSerializer.Serialize(req);
        Assert.Contains("external_id", json);
        Assert.DoesNotContain("conflict_policy", json);
    }

    [Theory]
    [InlineData("error")]
    [InlineData("match")]
    [InlineData("replace")]
    public void AcceptsAllConflictPolicies(string policy)
    {
        var req = new CreateNodeRequest
        {
            Labels = new List<string> { "N" },
            Properties = new Dictionary<string, object?>(),
            ExternalId = "str:x",
            ConflictPolicy = policy,
        };

        var json = JsonSerializer.Serialize(req);
        Assert.Contains(policy, json);
    }
}

public class GetNodeByExternalIdResponseDeserializationTests
{
    [Fact]
    public void DeserializesNodePresent()
    {
        var raw = """
            {
              "node": {"id": "42", "labels": ["Person"], "properties": {}},
              "message": "found"
            }
            """;

        var resp = JsonSerializer.Deserialize<GetNodeByExternalIdResponse>(raw)!;
        Assert.NotNull(resp.Node);
        Assert.Equal("42", resp.Node!.Id);
        Assert.Equal("found", resp.Message);
        Assert.Null(resp.Error);
    }

    [Fact]
    public void DeserializesNodeAbsent()
    {
        var raw = """{"node": null, "message": "not found"}""";
        var resp = JsonSerializer.Deserialize<GetNodeByExternalIdResponse>(raw)!;
        Assert.Null(resp.Node);
        Assert.Equal("not found", resp.Message);
    }
}

public class ExternalIdUrlEncodingTests
{
    [Theory]
    [InlineData("str:hello world", "str%3Ahello%20world")]
    [InlineData("sha256:deadbeef", "sha256%3Adeadbeef")]
    [InlineData("uuid:550e8400-e29b-41d4-a716-446655440000", "uuid%3A550e8400-e29b-41d4-a716-446655440000")]
    public void EscapeDataStringHandlesPrefixedForms(string input, string expectedEncoded)
    {
        var encoded = Uri.EscapeDataString(input);
        Assert.Equal(expectedEncoded, encoded);
    }
}
