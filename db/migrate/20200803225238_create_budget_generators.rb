class CreateBudgetGenerators < ActiveRecord::Migration[5.2]
  def change
    create_table :budget_generators do |t|
      t.integer :user_id, null: false
      t.timestamps
    end
  end
end
