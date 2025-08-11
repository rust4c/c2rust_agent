from src.base.Base import Base

from src.modules.AgentServer.ConvertService import ConvertService
from src.modules.DatebaseServer.DatabaseManager import DatabaseManager

from pathlib import Path

class test(Base):
    def __init__(self):
        super().__init__()

    def run(self):
        config = self.load_config()
        print(config.get("llm", {}).get("target_platform"))

def main():
    print("Hello from c2rust-agent!")
    test_ = test()
    test_.run()
    db_client = DatabaseManager("/Users/peng/Documents/AppCode/Python/c2rust_agent/relation_analysis.db")
    convert = ConvertService(db_client,
                            Path("/Users/peng/Documents/AppCode/Python/c2rust_agent/test_file"))
    convert.convert_singles_file()

if __name__ == "__main__":
    main()
