class Dropdb < ActiveRecord::Migration[5.2]
  def change
    drop_table :budget_generators
  end
end
