#include <cppunit/extensions/HelperMacros.h>
#include <source.hh>

class SourceTest : public CppUnit::TestFixture
{
	CPPUNIT_TEST_SUITE(SourceTest);
	CPPUNIT_TEST(testProjection);
	CPPUNIT_TEST_SUITE_END();

public:
	void testProjection(void)
	{
		using bytes = std::vector<uint8_t>;

		po::address_space base_as("base",po::rrange(0,128),std::function<bytes(const bytes&)>());
		po::address_space xor_as("xor",po::rrange(0,64),std::function<bytes(const bytes&)>());
		po::address_space add_as("add",po::rrange(0,27),std::function<bytes(const bytes&)>());
		po::address_space zlib_as("zlib",po::rrange(0,128),std::function<bytes(const bytes&)>());
		po::address_space aes_as("aes",po::rrange(0,32),std::function<bytes(const bytes&)>());
		po::graph<po::address_space,po::rrange> g;

		auto base_vx = g.insert_node(base_as);
		auto xor_vx = g.insert_node(xor_as);
		auto add_vx = g.insert_node(add_as);
		auto zlib_vx = g.insert_node(zlib_as);
		auto aes_vx = g.insert_node(aes_as);

		g.insert_edge(po::rrange(0,64),xor_vx,base_vx);
		g.insert_edge(po::rrange(64,72),add_vx,base_vx);
		g.insert_edge(po::rrange(45,64),add_vx,xor_vx);
		g.insert_edge(po::rrange(80,128),zlib_vx,base_vx);
		g.insert_edge(po::rrange(32,64),aes_vx,zlib_vx);

		auto proj = po::projection(base_as,g);
		for(const std::pair<po::rrange,po::address_space> &p: proj)
			std::cerr << p.first << " => " << p.second.name << std::endl;
	}
};
