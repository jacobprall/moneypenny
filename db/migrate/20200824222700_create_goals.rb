class CreateGoals < ActiveRecord::Migration[5.2]
  def change
    create_table :goals do |t|
      t.integer :account_id, null: false
      t.string :goal_category, null: false
      t.string :title, null: false 
      t.float :goal_amount, null: false 
      t.timestamps
    end
  end
end
