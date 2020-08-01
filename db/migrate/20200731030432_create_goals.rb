class CreateGoals < ActiveRecord::Migration[5.2]
  def change
    create_table :goals do |t|
      t.string :goal_category, null: false
      t.string :title, null: false
      t.string :notes
      t.integer :goal_amt, null: false
      t.integer :account_id, null: false
      t.boolean :completed, null: false, default: false
      t.timestamps
    end
  end
end
