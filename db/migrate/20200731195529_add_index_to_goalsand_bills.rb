class AddIndexToGoalsandBills < ActiveRecord::Migration[5.2]
  def change
    add_index :goals, [:account_id]
   
  end
end
