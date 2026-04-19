using MessagePack;
using Nexus.SDK.Transports;
using Xunit;

namespace Nexus.SDK.Tests;

public class EndpointTests
{
    [Fact]
    public void DefaultLocalIsNexusLoopback()
    {
        var ep = Endpoint.DefaultLocal();
        Assert.Equal("nexus", ep.Scheme);
        Assert.Equal("127.0.0.1", ep.Host);
        Assert.Equal(Endpoint.RpcDefaultPort, ep.Port);
        Assert.Equal("nexus://127.0.0.1:15475", ep.ToString());
    }

    [Fact]
    public void ParseNexusWithExplicitPort()
    {
        var ep = Endpoint.Parse("nexus://example.com:17000");
        Assert.Equal("nexus", ep.Scheme);
        Assert.Equal(17000, ep.Port);
    }

    [Fact]
    public void ParseHttpDefaultPort()
    {
        var ep = Endpoint.Parse("http://localhost");
        Assert.Equal("http", ep.Scheme);
        Assert.Equal(Endpoint.HttpDefaultPort, ep.Port);
    }

    [Fact]
    public void ParseHttpsDefaultPort()
    {
        var ep = Endpoint.Parse("https://nexus.example.com");
        Assert.Equal("https", ep.Scheme);
        Assert.Equal(Endpoint.HttpsDefaultPort, ep.Port);
    }

    [Fact]
    public void ParseBareIsRpc()
    {
        var ep = Endpoint.Parse("10.0.0.5:15600");
        Assert.Equal("nexus", ep.Scheme);
        Assert.Equal(15600, ep.Port);
    }

    [Fact]
    public void ParseIPv6()
    {
        var ep = Endpoint.Parse("nexus://[::1]:15475");
        Assert.Equal("::1", ep.Host);
        Assert.Equal(15475, ep.Port);
    }

    [Fact]
    public void RejectsNexusRpcScheme()
    {
        var ex = Assert.Throws<ArgumentException>(() => Endpoint.Parse("nexus-rpc://host"));
        Assert.Contains("unsupported URL scheme", ex.Message);
    }

    [Fact]
    public void RejectsEmpty()
    {
        Assert.Throws<ArgumentException>(() => Endpoint.Parse(""));
        Assert.Throws<ArgumentException>(() => Endpoint.Parse("   "));
    }

    [Fact]
    public void AsHttpUrlSwapsRpcToSiblingPort()
    {
        var ep = Endpoint.Parse("nexus://host:17000");
        Assert.Equal("http://host:15474", ep.AsHttpUrl());
    }
}

public class WireCodecTests
{
    [Fact]
    public void EncodesNullAsLiteralString()
    {
        Assert.Equal("Null", Codec.ToWire(NexusValue.Null()));
    }

    [Fact]
    public void EncodesStrAsTaggedMap()
    {
        var wire = Codec.ToWire(NexusValue.Str("hi"));
        var m = Assert.IsType<Dictionary<object, object?>>(wire);
        Assert.Equal("hi", m["Str"]);
    }

    [Fact]
    public void RoundtripsPrimitives()
    {
        var cases = new[]
        {
            NexusValue.Null(),
            NexusValue.Bool(true),
            NexusValue.Bool(false),
            NexusValue.Int(0),
            NexusValue.Int(-42),
            NexusValue.Str(""),
            NexusValue.Str("hello"),
            NexusValue.Float(1.5),
        };
        foreach (var input in cases)
        {
            var back = Codec.FromWire(Codec.ToWire(input));
            Assert.Equal(input.Kind, back.Kind);
        }
    }

    [Fact]
    public void RoundtripsNestedArrayAndMap()
    {
        var v = NexusValue.Map(new List<(NexusValue, NexusValue)>
        {
            (NexusValue.Str("labels"), NexusValue.Array(new List<NexusValue> { NexusValue.Str("Person") })),
            (NexusValue.Str("age"), NexusValue.Int(30)),
        });
        var back = Codec.FromWire(Codec.ToWire(v));
        Assert.Equal(NexusValueKind.Map, back.Kind);
    }

    [Fact]
    public void RejectsMultiKeyTaggedValue()
    {
        var ex = Assert.Throws<InvalidOperationException>(() =>
            Codec.FromWire(new Dictionary<object, object?> { ["Str"] = "a", ["Int"] = 1L }));
        Assert.Contains("single-key", ex.Message);
    }

    [Fact]
    public void RequestFrameHasU32LELengthPrefix()
    {
        var frame = Codec.EncodeRequestFrame(new Codec.RpcRequest { Id = 7, Command = "PING" });
        var length = (uint)(frame[0] | (frame[1] << 8) | (frame[2] << 16) | (frame[3] << 24));
        Assert.Equal((uint)(frame.Length - 4), length);
        Assert.True(length > 0);
    }

    [Fact]
    public void DecodeOkResponse()
    {
        var body = MessagePackSerializer.Typeless.Serialize(new Dictionary<object, object?>
        {
            ["id"] = 9u,
            ["result"] = new Dictionary<object, object?>
            {
                ["Ok"] = new Dictionary<object, object?> { ["Str"] = "OK" },
            },
        });
        var resp = Codec.DecodeResponseBody(body);
        Assert.Equal(9u, resp.Id);
        Assert.True(resp.Ok);
        Assert.Equal("OK", resp.Unwrap().AsString());
    }

    [Fact]
    public void DecodeErrResponse()
    {
        var body = MessagePackSerializer.Typeless.Serialize(new Dictionary<object, object?>
        {
            ["id"] = 3u,
            ["result"] = new Dictionary<object, object?> { ["Err"] = "boom" },
        });
        var resp = Codec.DecodeResponseBody(body);
        Assert.False(resp.Ok);
        Assert.Equal("boom", resp.Err);
        var ex = Assert.Throws<InvalidOperationException>(() => resp.Unwrap());
        Assert.Contains("boom", ex.Message);
    }
}

public class CommandMapTests
{
    [Fact]
    public void CypherSimple()
    {
        var m = CommandMap.Map("graph.cypher", new Dictionary<string, object?> { ["query"] = "RETURN 1" });
        Assert.NotNull(m);
        Assert.Equal("CYPHER", m!.Command);
        Assert.Single(m.Args);
        Assert.Equal("RETURN 1", m.Args[0].AsString());
    }

    [Fact]
    public void CypherWithParams()
    {
        var m = CommandMap.Map("graph.cypher", new Dictionary<string, object?>
        {
            ["query"] = "MATCH (n {name:$n}) RETURN n",
            ["parameters"] = new Dictionary<string, object?> { ["n"] = "Alice" },
        });
        Assert.NotNull(m);
        Assert.Equal(2, m!.Args.Count);
        Assert.Equal(NexusValueKind.Map, m.Args[1].Kind);
    }

    [Theory]
    [InlineData("graph.ping")]
    [InlineData("graph.stats")]
    [InlineData("graph.health")]
    [InlineData("graph.quit")]
    public void NoArgVerbs(string name)
    {
        var m = CommandMap.Map(name, new Dictionary<string, object?>());
        Assert.NotNull(m);
        Assert.Empty(m!.Args);
    }

    [Fact]
    public void AuthApiKeyWins()
    {
        var m = CommandMap.Map("auth.login", new Dictionary<string, object?>
        {
            ["api_key"] = "nx_1",
            ["username"] = "u",
            ["password"] = "p",
        });
        Assert.NotNull(m);
        Assert.Single(m!.Args);
    }

    [Fact]
    public void AuthFallsBackToUserPass()
    {
        var m = CommandMap.Map("auth.login", new Dictionary<string, object?>
        {
            ["username"] = "u",
            ["password"] = "p",
        });
        Assert.NotNull(m);
        Assert.Equal(2, m!.Args.Count);
    }

    [Fact]
    public void DbCreateRequiresName()
    {
        Assert.Null(CommandMap.Map("db.create", new Dictionary<string, object?>()));
        var m = CommandMap.Map("db.create", new Dictionary<string, object?> { ["name"] = "mydb" });
        Assert.NotNull(m);
        Assert.Equal("DB_CREATE", m!.Command);
    }

    [Fact]
    public void DataImportRequiresBoth()
    {
        Assert.Null(CommandMap.Map("data.import", new Dictionary<string, object?> { ["format"] = "json" }));
        Assert.Null(CommandMap.Map("data.import", new Dictionary<string, object?> { ["data"] = "[]" }));
        var m = CommandMap.Map("data.import", new Dictionary<string, object?>
        {
            ["format"] = "json",
            ["data"] = "[]",
        });
        Assert.NotNull(m);
    }

    [Fact]
    public void UnknownReturnsNull()
    {
        Assert.Null(CommandMap.Map("graph.nonsense", new Dictionary<string, object?>()));
    }
}

public class TransportModeParserTests
{
    [Theory]
    [InlineData("nexus", TransportMode.NexusRpc)]
    [InlineData("rpc", TransportMode.NexusRpc)]
    [InlineData("NEXUSRPC", TransportMode.NexusRpc)]
    [InlineData("http", TransportMode.Http)]
    [InlineData("https", TransportMode.Https)]
    [InlineData("resp3", TransportMode.Resp3)]
    public void ParsesCanonicalAndAliases(string input, TransportMode expected)
    {
        Assert.Equal(expected, TransportModeParser.Parse(input));
    }

    [Theory]
    [InlineData("")]
    [InlineData("  ")]
    [InlineData("auto")]
    [InlineData("widget")]
    public void ReturnsNullForEmptyAutoAndUnknown(string input)
    {
        Assert.Null(TransportModeParser.Parse(input));
    }

    [Fact]
    public void ReturnsNullForNull()
    {
        Assert.Null(TransportModeParser.Parse(null));
    }
}

public class TransportFactoryTests
{
    [Fact]
    public async Task DefaultIsRpc()
    {
        var built = TransportFactory.Build(new TransportBuildOptions { EnvTransport = "" }, new Credentials());
        Assert.Equal(TransportMode.NexusRpc, built.Mode);
        Assert.Equal(Endpoint.RpcDefaultPort, built.Endpoint.Port);
        await built.Transport.DisposeAsync();
    }

    [Fact]
    public async Task UrlSchemeWinsOverEnv()
    {
        var built = TransportFactory.Build(
            new TransportBuildOptions { BaseUrl = "http://host:15474", EnvTransport = "nexus" },
            new Credentials());
        Assert.Equal(TransportMode.Http, built.Mode);
        await built.Transport.DisposeAsync();
    }

    [Fact]
    public async Task EnvOverridesBareHost()
    {
        var built = TransportFactory.Build(
            new TransportBuildOptions { BaseUrl = "host:15474", EnvTransport = "http" },
            new Credentials());
        Assert.Equal(TransportMode.Http, built.Mode);
        await built.Transport.DisposeAsync();
    }

    [Fact]
    public async Task ConfigHintHonouredOnBareUrl()
    {
        var built = TransportFactory.Build(
            new TransportBuildOptions { BaseUrl = "host:15474", Transport = TransportMode.Http, EnvTransport = "" },
            new Credentials());
        Assert.Equal(TransportMode.Http, built.Mode);
        await built.Transport.DisposeAsync();
    }

    [Fact]
    public void Resp3RaisesClearError()
    {
        var ex = Assert.Throws<ArgumentException>(() =>
            TransportFactory.Build(
                new TransportBuildOptions { Transport = TransportMode.Resp3, EnvTransport = "" },
                new Credentials()));
        Assert.Contains("resp3 transport is not yet shipped", ex.Message);
    }
}

public class CredentialsTests
{
    [Fact]
    public void EmptyHasNone() => Assert.False(new Credentials().HasAny());

    [Fact]
    public void ApiKeySetsFlag() => Assert.True(new Credentials { ApiKey = "k" }.HasAny());

    [Fact]
    public void UsernameAloneDoesNotCount() =>
        Assert.False(new Credentials { Username = "u" }.HasAny());

    [Fact]
    public void UserAndPassCount() =>
        Assert.True(new Credentials { Username = "u", Password = "p" }.HasAny());
}

public class RpcTransportFailsFastTests
{
    [Fact]
    public async Task FailsFastOnUnreachableHost()
    {
        var ep = new Endpoint("nexus", "127.0.0.1", 1); // reserved port
        await using var t = new RpcTransport(ep, new Credentials(), TimeSpan.FromMilliseconds(500));
        var ex = await Assert.ThrowsAnyAsync<Exception>(async () =>
            await t.CallAsync("PING", new List<NexusValue>()));
        Assert.True(ex is IOException || ex is OperationCanceledException,
            $"unexpected exception type: {ex.GetType().Name}");
    }
}
